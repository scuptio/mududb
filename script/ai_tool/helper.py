import re
import subprocess
from typing import List, Tuple


def parse_markdown_code_blocks(markdown_text: str) -> List[Tuple[str, str]]:
    """
    Parse code blocks and their language tags from markdown text with better handling.

    Args:
        markdown_text (str): The markdown text to parse

    Returns:
        List[Tuple[str, str]]: A list of tuples containing (language, code_content)
                               for each code block found
    """
    code_blocks = []

    # More robust pattern that handles various edge cases
    pattern = r'^```(\w*)\n(.*?)^```'
    print(markdown_text)
    # Find all matches
    matches = re.finditer(pattern, markdown_text, re.DOTALL | re.M)

    for match in matches:
        language = match.group(1).strip() if match.group(1) else ""
        code_content = match.group(2).strip()
        code_blocks.append((language, code_content))

    return code_blocks


def gen_object_source_code(ddl_sql_path: str, output_dir: str):
    try:
        result = subprocess.run(
            [
                "mgen",
                "--in-path", ddl_sql_path,
                "--lang", "rust",
                "--out-path", output_dir,
            ],
            capture_output=True,
            text=True,
            check=True)
        print(f"mgen output: {result.stdout.strip()}")
    except subprocess.CalledProcessError as e:
        print(f"execute mgen command failed: {e}")
