#!/usr/bin/env python3
"""Render an Android manifest template from APP_* environment settings."""
import os
import re
import sys
from pathlib import Path
from xml.sax.saxutils import escape


def render(template, values):
    def replace(match):
        key = match.group(1)
        return escape(values[key], {'"': "&quot;", "'": "&apos;"})
    return re.sub(r"@(APP_[A-Z_]+)@", replace, template)


if __name__ == "__main__":
    Path(sys.argv[2]).write_text(render(Path(sys.argv[1]).read_text(), os.environ))
