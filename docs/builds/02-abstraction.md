# Abstraction and libraries

It is often hard to read when all invocations of the compiler is interleaved in with the files to build. It is therefore useful to create reusable patterns and conventions for how to build.

For example, to build a C file, and generate an object file, will be done often.

Since Lead can wrap that behaviour in a function, it is possible to reuse, and wrap in a function:

Updating the example from previous chapter with two functions `cc` and `link` gives:

```lead
|{pb, cwd, ...}|
let
  cc_rule = pb.rule (|{input, output, ...}| {
      command = ["gcc", "-c", "-o", output, "-MMD", "-MF", "${output}.d", input];
      depfile = "${output}.d";
  });

  link_rule = pb.rule (|{input, output, ...}| {
      command = ["gcc", "-o", output, input];
  });

  cc = |src obj| pb.build {
    rule = cc_rule;
    input = [src];
    output = obj;
  };

  link = |output objs| pb.build {
    rule = link_rule;
    input = objs;
    output = output;
  };

  objs = [
    (cc (cwd / "src" / "main.c") (cwd / "build" / "main.o"));
  ];

  app = link (cwd / "build" / "app") objs;
in
  app
```

The above example shows that it can significantly reduce the amount of overhead per file, but we can go further.

## Creating library functions

Remember the rewriting rules available in the [chapter about paths](../language/05-paths.md)? Put that togehter with how binding the first arguments of a function, as descibed in the [chapter about functions](../language/03-functions.md).

That results in the following code:

```lead
|{pb, cwd, ...}|
let
  gcc_target = |{gcc_prefix, extra_cc_args, extra_ld_args}| {
    cc_rule = pb.rule (|{input, output, ...}| {
        command = ["${gcc_prefix}gcc", "-c", "-o", output, "-MMD", "-MF", "${output}.d", input] ++ extra_cc_args;
        depfile = "${output}.d";
    });

    link_rule = pb.rule (|{input, output, ...}| {
        command = ["${gcc_prefix}gcc", "-o", output, input] ++ extra_ld_args;
    });
  };

  remap_c_to_o = |srcdir objdir file|
    pb.translate {
        input = pb.retype {
            input = file;
            from = ".c";
            to = ".o";
        };
        from = srcdir;
        to = objdir;
    };

  lang_c_rules = |target srcdir objdir| {
    cc = |src| pb.build {
      rule = target.cc_rule;
      input = [src];
      output = remap_c_to_o srcdir objdir src;
    };

    link = |output objs| (pb.build {
      rule = target.link_rule;
      input = objs;
      output = output;
    });
  };

  native_c_target = {
    gcc_prefix = "";
    extra_cc_args = [];
    extra_ld_args = [];
  };

  cross_target = {
    gcc_prefix = "riscv64-linux-gnu-";
    extra_cc_args = [];
    extra_ld_args = [];
  };


  srcdir = cwd / "src";
  objdir = cwd / "build";

  native = lang_c_rules 
    (gcc_target native_c_target)
    srcdir (objdir / "native");

  cross = lang_c_rules 
    (gcc_target native_c_target)
    srcdir (objdir / "cross");

  srcs = [
    (cwd / "src" / "main.c")
  ];

in
  [
    native.link (cwd / "app") [ |src| native.cc src for srcs ],
    cross.link (cwd / "cross-app") [ |src| cross.cc src for srcs ],
  ]
```

Which generates a `build.ninja` as:
```
rule gcc_o
  command = gcc -o ${out} ${in}

rule gcc_c_o_MMD
  command = gcc -c -o ${out} -MMD -MF ${out}.d ${in}
  depfile = ${out}.d

rule gcc_o1
  command = gcc -o ${out} ${in}

rule gcc_c_o_MMD1
  command = gcc -c -o ${out} -MMD -MF ${out}.d ${in}
  depfile = ${out}.d

build build/native/main.o: gcc_c_o_MMD src/main.c

build app: gcc_o build/native/main.o

build build/cross/main.o: gcc_c_o_MMD1 src/main.c

build cross-app: gcc_o1 build/cross/main.o

```

This example shows how to separate:
- How to invoke the compiler
- The configuration of the compiler, with which targets
- And what sources depends on which.

It also shows that there are no globals, which means the there is no issue having concurrent targets with different paramters. This is especially important in cases where:
- managing code generation, for example parser generators
- building test suites, using different flags and output directories
- building for multiple embedded targets, using variants of compilers and flags
- reusing libraries for multiple output binaries, where libraries has its own set of flags
- mixing languages, where one library is built before the other
- and many more cases

## Seprating libraries into multiple files

This becomes even more powerful when separating the different parts into differnet files.

Lets first wrap the language behaviour into a file `lib/lang/c.pbb`:
```lead
|{pb, ...}|
|target srcdir objdir|
let
  {gcc_prefix, extra_cc_args, extra_ld_args, ...} = target;

  cc_rule = pb.rule (|{input, output, ...}| {
      command = ["${gcc_prefix}gcc", "-c", "-o", output, "-MMD", "-MF", "${output}.d", input] ++ extra_cc_args;
      depfile = "${output}.d";
  });

  link_rule = pb.rule (|{input, output, ...}| {
      command = ["${gcc_prefix}gcc", "-o", output, input] ++ extra_ld_args;
  });

  remap_c_to_o = |srcdir objdir file|
    pb.translate {
        input = pb.retype {
            input = file;
            from = ".c";
            to = ".o";
        };
        from = srcdir;
        to = objdir;
    };
in
{
    cc = |src| pb.build {
      rule = cc_rule;
      input = [src];
      output = remap_c_to_o srcdir objdir src;
    };

    link = |output objs| (pb.build {
      rule = link_rule;
      input = objs;
      output = output;
    });
}
```

next, define the set of targets available, one per file:

`lib/targets/native.pbb`:

```lead
|{...}|
{
  gcc_prefix = "";
  extra_cc_args = [];
  extra_ld_args = [];
}
```

and `lib/targets/riscv64`:
```lead
|{...}|
{
  gcc_prefix = "risvc64-linux-gnu-";
  extra_cc_args = [];
  extra_ld_args = [];
}
```

Our app will then be `main.pbb`:

```lead
|{cwd, include, ...}|
let
  lang_c = include (cwd / "lib" / "lang" / "c.pbb");

  targets = [ "native", "riscv64" ];

  srcdir = cwd / "src";
  objdir = cwd / "build";

  srcs = [
    (cwd / "src" / "main.c"),
    (cwd / "src" / "lib.c")
  ];

  build = |target| (
    let
      target_desc = include (cwd / "lib" / "targets" / "${target}.pbb");
      { cc, link } = lang_c target_desc srcdir (objdir / target);
    in
      (link (cwd / "app-${target}") [ |src| cc src for srcs ])
  );
in
  ([ |target| build target for targets ])
```

We have now crated a working library implementation, where compiler, target and application specificaion is separated.

## Standard libraries and reusability

For reusability, there is a library, [lead-lib](https://github.com/lead-build/lead-lib/) that provides a set of tools and abstractions, so the code above doesn't need to be part of the build specification, but still keep the pureness, without any implicit functionality.