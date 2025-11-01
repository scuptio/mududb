# How to Start
A wallet application example.

## Clone the Repository

```bash
git clone https://github.com/scuptio/mududb.git
```
## Install Tools and MuduDB Server

```bash

cd mududb/mpk && cargo install --path .
cd mududb/mudu_gen && cargo install --path .
cd mududb/mudud && cargo install --path .
```

## Prompt Engineering and Create Project

### Prompting

Configure the `ARK_API_KEY` environment variable for the Volcengine SDK
and run the Mudu AI-Aided Development Tool to get started with vibe coding.

```bash
python mudu_aad.py --doc_dir ../
```

This will generate a response and retrieve the DDL SQL, ER diagram, and procedure source code.
Generating Code

Generate objects from the DDL SQL using mudu_gen:

```bash

mudu_gen --in_path [Some DDL SQL file]
````

You will then have the project.

### Project Structure

```bash
tree
```

```bash output
.
.
├── Cargo.lock
├── Cargo.toml
│   └── setup.sh
├── er
│   └── wallet_er.plantuml
├── makefile.toml
├── readme.md
├── src
│   ├── lib.rs
│   ├── rust
│   │   ├── item.rs
│   │   ├── mod.rs
│   │   ├── orders.rs
│   │   ├── procedures.rs
│   │   ├── transactions.rs
│   │   ├── users.rs
│   │   ├── wallets.rs
│   │   └── warehouse.rs
│   ├── sql
│   │   ├── ddl.sql
│   │   └── init.sql
│   └── toml
│       ├── app.cfg.toml
│       └── procedure.desc.toml
├── mpk
    └── wallet.mpk
└── wasm
    └── example.wasm
```


### TODO ..
Some manual work is required here. Make it better.

## Generate package
```bash
cargo make
```

This will build the package.
Start MuduDB Server
Configure MuduDB

Edit the configuration file:
```bash
cat ${HOME}/.mudu/mududb_cfg.toml
```

Example output:
```toml
mpk_path = [mudu package directory]
data_path = [mudu database file directory]
listen_ip = [server ip]
listen_port = [listen port]
```
Copy the wallet.mpk file to the Mudu package directory as specified in the mududb_cfg.toml file.

## Start the Server

```bash
mudud
```

MuduDB will now serve the example wallet app with a web interface API.
