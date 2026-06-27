# Rules and Builds

Rules and builds are special value types in Lead. They are not ordinary literals; they are created using the builtin constructors `pb.rule` and `pb.build`.

## Rules

A rule specifies how to transform inputs into outputs. Create a rule with the builtin constructor `pb.rule` by providing a function that declares the variables the rule uses (for example `|{input, output, ...}|`) and returns an object describing the rule template and metadata.

Common fields returned by that object:
- `command` - the command template, typically a list of command-line arguments (for example `["gcc", "-c", "-o", output, "-MMD", "-MF", "${output}.d", input]`) or a string template.
- `description` - short, human-readable text shown in build output.
- Optional fields such as `depfile`, `rspfile`, `pool`, etc., which control additional Ninja features.

The variables named in the rule function become placeholders used when emitting the Ninja rule; pb maps those placeholders to Ninja variables (e.g. `$in` / `$out`) as part of the translation. pb also generates a stable, unique rule name derived from the rule's template.

When the rule is emitted to Ninja, the Lead variables `input` and `output` are mapped to the Ninja placeholders `$in` and `$out`; this is necessary because `in` is a reserved keyword in Lead.

A simple example, which compiles a file to an object file with dependency tracking:

```
rule_compile = pb.rule |{input, output, ...}|
    {
        command = ["gcc", "-c", "-o", output, "-MMD", "-MF", "${output}.d", input];
        depfile = "${output}.d";
    };
```

will generate a rule:
```
rule gcc_c_o_MMD
  command = gcc -c -o ${out} -MMD -MF ${out}.d ${in}
  depfile = ${out}.d
```

where the name of the rule is generated from the command, and is guaranteed to be unique

## Builds

A build is an instance of a rule applied to concrete inputs and outputs. In Lead, a build is constructed with `pb.build` and references a rule plus the relevant inputs, outputs, and parameters, which will generate a `build` statement in the output `ninja.build`.

When constructing a build, set the following fields:

- `rule` - the rule object produced by `pb.rule`
- `input` - the file or files needed by the build
- `output` - the file or files produced by the build

Minimal example:

```lead
|{pb, cwd, ...}|
let
  cc_rule = pb.rule (|{input, output, ...}| {
    command = ["gcc", "-o", output, "-MMD", "-MF", "${output}.d", input];
    depfile = "${output}.d";
  });

  my_build = pb.build {
    rule = cc_rule;
    input = [cwd / "src" / "main.c"];
    output = cwd / "build" / "main.o";
  };
in
  my_build
```

Generating a `build.ninja` file as:

```ninja
rule gcc_o_MMD_MF
  command = gcc -o ${out} -MMD -MF ${out}.d ${in}
  depfile = ${out}.d

build build/main.o: gcc_o_MMD_MF src/main.c
```

## Dependency chains

The output produced by one `pb.build` can be used as the input to another `pb.build`. This creates a dependency chain, so the generated build graph ensures that the earlier build completes before the later one runs.

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

  objs = [
    (pb.build {
      rule = cc_rule;
      input = [cwd / "src" / "main.c"];
      output = cwd / "build" / "main.o";
    })
  ];

  app = pb.build {
    rule = link_rule;
    input = objs;
    output = cwd / "build" / "app";
  };
in
  app
```

generating a build.ninja file as:
```ninja
rule gcc_o
  command = gcc -o ${out} ${in}

rule gcc_c_o_MMD
  command = gcc -c -o ${out} -MMD -MF ${out}.d ${in}
  depfile = ${out}.d

build build/main.o: gcc_c_o_MMD src/main.c

build build/app: gcc_o build/main.o
```

In this pattern, the first build becomes an input to the second build, forming a simple build dependency chain.

In this chapter have looked at the conepts of how to construct builds. In the next chapter we will look at how to use the language constructs to separate *how* to build from *what* to build.