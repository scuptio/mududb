import unittest

# Import the functions to test (assuming they are in the same module)
from .helper import parse_markdown_code_blocks, gen_object_source_code


class TestMarkdownCodeBlockParser(unittest.TestCase):
    """
    Test cases for markdown code block parsing functions.
    """

    def setUp(self):
        """Set up common test data."""
        self.simple_python_code = """```python
def hello():
    print("Hello World")
    return True
```"""

        self.simple_javascript_code = """```javascript
function test() {
    console.log("test");
}
```"""

        self.no_language_code = """```
plain text code block
multiple lines
```"""

        self.html_code = """```html
<!DOCTYPE html>
<html>
<body>
    <h1>Test</h1>
</body>
</html>
```"""

    def test_basic_python_code_block(self):
        """Test parsing a basic Python code block."""
        markdown = f"""
Some text before
{self.simple_python_code}
Some text after
"""
        result = parse_markdown_code_blocks(markdown)
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0][0], "python")
        self.assertIn("def hello():", result[0][1])
        self.assertIn('print("Hello World")', result[0][1])

    def test_basic_javascript_code_block(self):
        """Test parsing a basic JavaScript code block."""
        markdown = f"""
Some text
{self.simple_javascript_code}
More text
"""
        result = parse_markdown_code_blocks(markdown)
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0][0], "javascript")
        self.assertIn("function test()", result[0][1])

    def test_no_language_code_block(self):
        """Test parsing a code block without language specification."""
        markdown = f"""
Text before
{self.no_language_code}
Text after
"""
        result = parse_markdown_code_blocks(markdown)
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0][0], "")  # Empty language
        self.assertIn("plain text code block", result[0][1])

    def test_multiple_code_blocks(self):
        """Test parsing multiple code blocks in one markdown."""
        markdown = f"""
Introduction
{self.simple_python_code}

Some explanation
{self.simple_javascript_code}

Conclusion
{self.no_language_code}
"""
        result = parse_markdown_code_blocks(markdown)
        self.assertEqual(len(result), 3)
        self.assertEqual(result[0][0], "python")
        self.assertEqual(result[1][0], "javascript")
        self.assertEqual(result[2][0], "")

    def test_indented_code_blocks_not_parsed(self):
        """Test that indented code blocks are NOT parsed."""
        markdown = """
Some text
    ```python
    print("indented - should not parse")
    ```
    
Regular code block:
```python
print("regular - should parse")
"""
        result = parse_markdown_code_blocks(markdown)
        self.assertEqual(len(result), 0)

    def test_plantuml_and_sql(self):
        markdown = """
# ER

```plantuml
@startuml
!theme plain

entity users {
  * user_id: INTEGER
  --
  phone_number: TEXT
  created_at: INTEGER
}

entity votes {
  * vote_id: TEXT
  --
  creator_id: INTEGER
  title: TEXT
  question: TEXT
  vote_type: TEXT
  max_choices: INTEGER
  end_time: INTEGER
  result_visibility: TEXT
  created_at: INTEGER
}

entity vote_options {
  * option_id: TEXT
  * vote_id: TEXT
  --
  option_text: TEXT
  created_at: INTEGER
}

entity user_votes {
  * record_id: TEXT
  --
  user_id: INTEGER
  vote_id: TEXT
  option_id: TEXT
  voted_at: INTEGER
  is_active: BOOLEAN
  created_at: INTEGER
}

users ||--o{ votes : "创建"
votes ||--o{ vote_options : "包含选项"
users ||--o{ user_votes : "投票"
vote_options ||--o{ user_votes : "被选择"
votes ||--o{ user_votes : "包含投票记录"

@enduml
```

# DDL SQL

```sql
-- Users table
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    phone_number TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);

-- Votes table
CREATE TABLE votes (
    vote_id TEXT PRIMARY KEY,
    creator_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    question TEXT NOT NULL,
    vote_type TEXT NOT NULL,
    max_choices INTEGER NOT NULL,
    end_time INTEGER NOT NULL,
    result_visibility TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Vote options table
CREATE TABLE vote_options (
    option_id TEXT PRIMARY KEY,
    vote_id TEXT NOT NULL,
    option_text TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- User votes table
CREATE TABLE user_votes (
    record_id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    vote_id TEXT NOT NULL,
    option_id TEXT NOT NULL,
    voted_at INTEGER NOT NULL,
    is_active BOOLEAN NOT NULL,
    created_at INTEGER NOT NULL
);
```        
        """
        result = parse_markdown_code_blocks(markdown)
        self.assertEqual(len(result), 2)
        self.assertEqual(result[0][0], "plantuml")
        self.assertEqual(result[1][0], "sql")

    def test_gen_source(self):
        gen_object_source_code("./backend/src/sql", "./backend/src/rust")
