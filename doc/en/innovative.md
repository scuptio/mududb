### 1. AI-Assisted Database Engineering

Uses large language models (LLMs) to accelerate database engineering by generating:

- **Entity-Relationship (ER) Diagrams**
- **Data Definition Language (DDL) Scripts**
- **Stored Procedures & Functions**

### 2. [Mudu Procedure (MP)](procedure.md)

Unifies two execution models:

- **Interactive execution** for ad hoc transactions
- **Procedural execution** for single-purpose transactional workflows

### 3. [Modern Hardware-Optimized Architecture](modern_hardware.md)

Improves hardware efficiency through:

- **Asynchronous I/O** optimized for NVMe SSDs
- **Cooperative concurrency** with no thread-switch overhead

### 4. Microkernel Architecture Design

Builds for modularity and extensibility:

- **Core Engine**: Handles only essential functions (storage, ACID, query parsing, query execution).
- **Plug-in Ecosystem**:
  - Extensions (for example, JSON or graph support)
  - External runtime modules (for example, ML inference)
  - Custom storage engines
