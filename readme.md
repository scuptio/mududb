# MuduDB  


 [<img src="doc/pic/mudu_logo.svg" width="10%">](doc/en/name.md)

[汉语](readme.cn.md)

---

MuduDB is database system primarily focused on OLTP (Online Transaction Processing).

**It is currently in an actively developing, early-stage phase for demonstration purposes only.**

It implements a range of innovative features designed to leverage modern AI and cloud computing technologies, aiming to significantly improve data system development efficiency and optimize resource utilization.

---

## Innovative Features of MuduDB  

### 1. AI-Assisted Database Engineering  
Accelerates development cycles by using large language models (LLMs) to generate:  
- **Entity-Relationship (ER) Diagrams**  
- **Data Definition Language (DDL) Scripts**  
- **Stored Procedures & Functions**  


### 2. [Mudu Procedure](doc/en/procedure.md)
Seamlessly integrates:  
- **Interactive** (for ad-hoc transaction)  
- **Procedural** (for one-shot transactions)  


### 3. Modern Hardware-Optimized Architecture  

Maximizes resource efficiency through:  
- **Asynchronous I/O** (optimized for NVMe/SSD)  
- **Cooperative Concurrency** (lightweight threading with near-zero overhead)  
  

### 4. Microkernel Architecture Design  
Embraces modularity and extensibility:  
- **Core Engine**: Handles only essential functions (storage, ACID, query parsing).  
- **Plug-in Ecosystem**:  
  - Extensions (e.g., JSON/Graph support)  
  - External Runtime Modules (e.g., ML inference)  
  - Custom Storage Engines  




## **Current Development Status** 

### Top-Down Development Approach  

#### Strategically prioritizing:  

1. **Developer Tooling** (AI tools, checking tools)  
2. **Frontend** (SQL APIs, ORM integrations, Runtime)  
3. **Core Engine** (Core transactional processing layer)  


#### Current Focus: Mudu Runtime  
- **Development Status**: Actively implementing the **Mudu Runtime** – a execution environment unifying procedural logic and interactive queries.  


## **Open-Source** 

Open-source release (Apache 2.0) targeted after core engine readiness.
