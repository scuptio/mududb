#!/usr/bin/env python3
from pathlib import Path

generated = Path(__file__).resolve().parents[1] / "generated" / "procedures.gen.ts"
text = generated.read_text(encoding="utf-8")
text = text.replace(
    'from "@mududb/assemblyscript-binding";',
    'from "../assembly/mududb_binding";',
)
generated.write_text(text, encoding="utf-8")
