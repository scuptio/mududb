## Requirements:

### Response with Markdown format

Please format source code using Markdown code blocks.

### DDL Specifications:

Use SQLite syntax with the following limitations:

The following database features are not supported:

1. Foreign Keys

2. Indexes

3. Auto-incrementing Keys

4. Constraints (except for the Primary Key constraint).

5. UNIQUE is not supported, do not use UNIQUE.
   Supported Data Types:

   integer

   long

   double

   text

Data Type Replacements & Conventions:

1. Timestamps: Must be represented using the i64 (64-bit integer) type.

2. Primary Keys: Use UUID strings instead of auto-incrementing keys.

3. Atomic Timestamps: Fields like created_at or updated_at must use an integer numeric type.

4. Boolean Values: Must be represented using the integer type (e.g., 0 for false, 1 for true).

### Backend Implementation

Technology: Rust

Implementation Details:

1. The core application logic will be implemented as Mudu procedures written in Rust.

2. Each procedure will be designed to handle a specific business operation.

### API Architecture

Backend Database: MuduDB

#### Communication Protocol:

1. The backend exposes a set of JSON APIs for the frontend to consume.

2. Each Mudu procedure serves as a dedicated JSON API endpoint.

#### Request & Response Format:

1. Request: The HTTP request body must be a JSON object containing the arguments for the target Mudu procedure.

2. Response: The HTTP response body will be the JSON representation of the procedure's return value.
