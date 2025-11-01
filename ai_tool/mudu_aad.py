"""
MuduDB AI assisted development tool
"""
import os
import sys
import httpx
import argparse
from datetime import datetime
from volcenginesdkarkruntime import Ark
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
client = Ark(
    # The output time of the reasoning model is relatively long. Please increase the timeout period.
    timeout=httpx.Timeout(timeout=1800),
)


def mudu_doc_content(directory):
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

    if doc_dir is None:
        doc_dir = '.'

    content_mudu =  mudu_doc_content(doc_dir)

    input_path = args.input_path
    if input_path is None:
        input_path = 'content.requirement.spec.md'
    now = datetime.now()
    formatted_time = now.strftime("__%Y_%m_%d_%H_%M_%S")
    file_output = f'ai_output_{formatted_time}.txt'

    content_input = content_mudu

    with open(input_path, 'r', encoding='utf-8') as file:
        content_raw = file.read()
        content_input = content_input + content_raw

    file_output = open(file_output, 'w', encoding='utf-8')

    # [Recommended] Streaming:
    print("----- streaming request -----")
    file_output.writelines(['----- Input Content -----', '===='])
    file_output.write(content_raw)

    file_output.writelines(['----- Request Output -----', '===='])
    stream = client.chat.completions.create(
        model="deepseek-r1-250528",
        messages=[
            {"role": "user", "content": content_input},
        ],
        stream=True
    )
    for chunk in stream:
        if not chunk.choices:
            continue
        if chunk.choices[0].delta.reasoning_content:
            file_output.write(chunk.choices[0].delta.reasoning_content)
            print(chunk.choices[0].delta.reasoning_content, end="")
        else:
            file_output.write(chunk.choices[0].delta.content)
            print(chunk.choices[0].delta.content, end="")
    print()
