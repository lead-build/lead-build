#!/bin/bash

if [ ! -d .venv ]; then
    python3 -m venv .venv
    .venv/bin/pip install -r docs/requirements.txt
fi

.venv/bin/mkdocs serve