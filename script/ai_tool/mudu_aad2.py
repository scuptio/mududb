"""
MuduDB AI assisted development tool
"""
import argparse
import os
import shutil
import sys
from datetime import datetime
from pathlib import Path

from pydantic import BaseModel
from volcenginesdkarkruntime import Ark

from helper import parse_markdown_code_blocks, gen_object_source_code

MODEL = "deepseek-v3-1-250821"


class SchemaDefine(BaseModel):
    er_diagram_plant_uml_string: str  # 步骤说明
    ddl_sql_string: str


class Session:
    def __init__(self, doc_directory, specification_path, output_path):
        self.model = MODEL
        self.specification_path = specification_path

        self.output_path = output_path
        self.backend_path = f"{output_path}/backend"
        # Authentication
        # 1.If you authorize your endpoint using an API key, you can set your api key to environment variable "ARK_API_KEY"
        # or specify api key by Ark(api_key="${YOUR_API_KEY}").
        # Note: If you use an API key, this API key will not be refreshed.
        # To prevent the API from expiring and failing after some time, choose an API key with no expiration date.
        # 2.If you authorize your endpoint with Volcengine Identity and Access Management（IAM),
        # set your api key to environment variable "VOLC_ACCESSKEY", "VOLC_SECRETKEY"
        # or specify ak&sk by Ark(ak="${YOUR_AK}", sk="${YOUR_SK}").
        # To get your ak&sk, please refer to this document(https://www.volcengine.com/docs/6291/65568)
        # For more information，please check this document（https://www.volcengine.com/docs/82379/1263279）
        api_key = os.environ.get("ARK_API_KEY")
        self.client = Ark(
            api_key=api_key,
            # base_url="https://ark.cn-beijing.volces.com/api/v3",
        )
        self.response = []
        self.response_id = None
        formatted_time = now.strftime("__%Y_%m_%d_%H_%M_%S")
        log_file_path = f'{output_path}/ai_output_{formatted_time}.txt'
        self.out_log_file = open(log_file_path, 'w', encoding='utf-8')
        self.specification_content = "The requirement specification of a Mudu App\n"
        self.document_content = document_content(doc_directory)
        with open(specification_path, 'r', encoding='utf-8') as file:
            self.specification_content += file.read()

    def prompt_document(self):
        response = self.client.responses.create(
            # model
            model=MODEL,
            input=[
                {"role": "system", "content": self.document_content}
            ],
            caching={"type": "enabled", "prefix": True},
            thinking={"type": "disabled"}
        )
        # the response
        print(response.usage.model_dump_json())
        self.out_log_file.write(response.usage.model_dump_json())
        self.response.append(response)

    def prompt_specification(self):
        if len(self.response) == 0:
            return
        response_prev = self.response[len(self.response) - 1]
        self.response_id = response_prev.id

        response = self.client.responses.create(
            model=self.model,
            previous_response_id=self.response_id,
            input=[
                {"role": "user", "content": self.specification_content},
            ],
            caching={"type": "enabled"},
            thinking={"type": "disabled"},
        )
        for output in response.output:
            for content in output.content:
                print(content.text)
                self.out_log_file.write(content.text)
        self.response_id = response.id
        self.response.append(response)

    def generate_schema(self):
        content = """
        Generate the DDL SQL and the ER diagram(using plantuml format) of the specified application
        """
        response_text = self.request_content(content)
        code_block_list = parse_markdown_code_blocks(response_text)
        for (lang, source) in code_block_list:
            if lang == "sql":
                write_text_to_file(f"{self.backend_path}/src/sql/ddl.sql", source)
            elif lang == "plantuml":
                write_text_to_file(f"{self.backend_path}/src/sql/er.plantuml", source)

    def generate_object(self):
        path_ddl_sql = f"{self.backend_path}/src/sql/ddl.sql"
        output_dir = f"{self.backend_path}/src/rust/"
        gen_object_source_code(path_ddl_sql, output_dir)

    def generate_procedure(self):
        content = "The followings are object file definition for each table schema.\n"

        path = Path(f"{self.backend_path}/src/rust")
        for item in path.rglob('*'):  # rglob for recursive search
            if item.is_file():
                name = item.name
                file_content = item.read_text(encoding="utf-8")
                content += f"source file name {name}, its content is: {file_content} \n"
            elif item.is_dir():
                print(f"Directory: {item}")
        content += "Use the object definition to generate all Mudu Procedure needed of the specified application."
        response_text = self.request_content(content)
        code_block_list = parse_markdown_code_blocks(response_text)
        procedure_source = ""
        for (i, (lang, source)) in enumerate(code_block_list):
            if lang == "rust":
                procedure_source += source
                if i + 1 != len(code_block_list):
                    procedure_source += "\n\n"
        write_text_to_file(f"{self.backend_path}/src/rust/procedure.rs", procedure_source)

    def generate_toml(self):
        content = ""
        content += "Generate .toml file for the project."
        response_text = self.request_content(content)
        code_block_list = parse_markdown_code_blocks(response_text)
        for (i, (lang, source)) in enumerate(code_block_list):
            if lang == "toml":
                write_text_to_file(f"{self.backend_path}/Cargo.toml", source)
                break

    def request_content(self, content: str):
        response = self.client.responses.create(
            model=self.model,
            previous_response_id=self.response_id,
            input=[
                {
                    "role": "user",
                    "content": content
                },
            ],
            caching={"type": "enabled"},
            thinking={"type": "disabled"},
        )
        response_text = response.output[0].content[0].text
        self.out_log_file.write(response_text + "\n")
        return response_text


def write_text_to_file(file_path, text, encoding='utf-8'):
    """
    Write text to a file at the specified path.
    Creates parent directories if they don't exist.

    Args:
        file_path (str or Path): Path to the file
        text (str): Text content to write
        encoding (str): File encoding, defaults to 'utf-8'
    """
    path = Path(file_path)
    # Create parent directories if they don't exist
    path.parent.mkdir(parents=True, exist_ok=True)
    # Write text to file
    path.write_text(text, encoding=encoding)


def document_content(directory):
    """
    :param directory
    :return: document merged string
    """
    if directory is None:
        return ''

    if not os.path.isdir(directory):
        raise ValueError(f"no such directory: {directory}")

    markdown_contents = []
    doc_content = "The following content is the documents of MuduDB and Mudu Procedure"
    prompt_content = "The following content is the requirements of a Mudu Application you want to develop"
    doc_path = []
    prompt_path = []
    for root, _, files in os.walk(directory):
        for filename in files:
            if filename.lower().endswith('.md'):
                filepath = os.path.join(root, filename)
                if filename.lower().endswith('.requirement.spec.md'):
                    continue
                elif filename.lower().endswith('.prompt.md'):
                    prompt_path.append(filepath)
                else:
                    doc_path.append(filepath)
    prompt_path.sort()
    for (i, array) in enumerate([doc_path, prompt_path]):
        if i == 0:
            markdown_contents.append(doc_content)
        else:
            markdown_contents.append(prompt_content)
        for path in array:
            try:
                with open(path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    markdown_contents.append(f"<!-- file: {path} -->")
                    markdown_contents.append(content)
                    markdown_contents.append("\n\n")
            except Exception as e:
                print(f"read file failed {path}: {str(e)}", file=sys.stderr)

    return ''.join(markdown_contents).rstrip('\n')


def command_exists(command):
    return shutil.which(command) is not None


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='MuduDB AI assisted development tool')
    parser.add_argument(
        '--doc_dir', type=str, required=False,
        help='document directory include *.md')
    parser.add_argument(
        '--input_path', type=str, required=False,
        help='input prompt file path')

    args = parser.parse_args()
    doc_dir = args.doc_dir

    if not command_exists("mgen"):
        print("command mgen not exist in path")
    if doc_dir is None:
        doc_dir = '.'

    input_path = args.input_path
    if input_path is None:
        input_path = 'content.requirement.spec.md'
    now = datetime.now()

    output_path = "./"
    session = Session(doc_dir, input_path, output_path)
    session.prompt_document()
    session.prompt_specification()
    session.generate_schema()
    session.generate_object()
    session.generate_procedure()
    session.generate_toml()
