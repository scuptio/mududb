module.exports = grammar({
    name: 'sql',

    extras: $ => [
        /\s\n/,
        /\s/,
        $.comment,
        $.marginalia,
    ],

    conflicts: $ => [
        [$.object_reference, $.qualified_field],
        [$.object_reference],
        [$.between_expression, $.binary_expression],
    ],

    precedences: $ => [
        [
            'binary_is',
            'unary_not',
            'binary_exp',
            'binary_times',
            'binary_plus',
            'binary_in',
            'binary_compare',
            'binary_relation',
            'binary_concat',
            'pattern_matching',
            'between',
            'clause_connective',
            'clause_disjunctive',
        ],
    ],

    word: $ => $._identifier,

    rules: {
        program: $ =>
            // any number of transactions, statements, or blocks with a terminating ;
            repeat(
                seq(
                    $.statement_transaction,
                    ';',
                ),
            ),

        keyword_select: _ => make_keyword("select"),
        keyword_delete: _ => make_keyword("delete"),
        keyword_insert: _ => make_keyword("insert"),
        keyword_copy: _ => make_keyword("copy"),
        keyword_replace: _ => make_keyword("replace"),
        keyword_update: _ => make_keyword("update"),
        keyword_truncate: _ => make_keyword("truncate"),
        keyword_merge: _ => make_keyword("merge"),
        keyword_into: _ => make_keyword("into"),
        keyword_overwrite: _ => make_keyword("overwrite"),
        keyword_values: _ => make_keyword("values"),
        keyword_value: _ => make_keyword("value"),
        keyword_matched: _ => make_keyword("matched"),
        keyword_set: _ => make_keyword("set"),
        keyword_from: _ => make_keyword("from"),
        keyword_left: _ => make_keyword("left"),
        keyword_right: _ => make_keyword("right"),
        keyword_inner: _ => make_keyword("inner"),
        keyword_full: _ => make_keyword("full"),
        keyword_outer: _ => make_keyword("outer"),
        keyword_cross: _ => make_keyword("cross"),
        keyword_join: _ => make_keyword("join"),
        keyword_lateral: _ => make_keyword("lateral"),
        keyword_on: _ => make_keyword("on"),
        keyword_where: _ => make_keyword("where"),
        keyword_order: _ => make_keyword("order"),
        keyword_group: _ => make_keyword("group"),
        keyword_partition: _ => make_keyword("partition"),
        keyword_by: _ => make_keyword("by"),
        keyword_having: _ => make_keyword("having"),
        keyword_desc: _ => make_keyword("desc"),
        keyword_asc: _ => make_keyword("asc"),
        keyword_limit: _ => make_keyword("limit"),
        keyword_offset: _ => make_keyword("offset"),
        keyword_primary: _ => make_keyword("primary"),
        keyword_create: _ => make_keyword("create"),
        keyword_alter: _ => make_keyword("alter"),
        keyword_change: _ => make_keyword("change"),
        keyword_analyze: _ => make_keyword("analyze"),
        keyword_explain: _ => make_keyword("explain"),
        keyword_verbose: _ => make_keyword("verbose"),
        keyword_modify: _ => make_keyword("modify"),
        keyword_drop: _ => make_keyword("drop"),
        keyword_add: _ => make_keyword("add"),
        keyword_table: _ => make_keyword("table"),
        keyword_tables: _ => make_keyword("tables"),
        keyword_view: _ => make_keyword("view"),
        keyword_column: _ => make_keyword("column"),
        keyword_columns: _ => make_keyword("columns"),
        keyword_materialized: _ => make_keyword("materialized"),
        keyword_tablespace: _ => make_keyword("tablespace"),
        keyword_sequence: _ => make_keyword("sequence"),
        keyword_increment: _ => make_keyword("increment"),
        keyword_minvalue: _ => make_keyword("minvalue"),
        keyword_maxvalue: _ => make_keyword("maxvalue"),
        keyword_none: _ => make_keyword("none"),
        keyword_owned: _ => make_keyword("owned"),
        keyword_start: _ => make_keyword("start"),
        keyword_restart: _ => make_keyword("restart"),
        keyword_key: _ => make_keyword("key"),
        keyword_as: _ => make_keyword("as"),
        keyword_distinct: _ => make_keyword("distinct"),
        keyword_constraint: _ => make_keyword("constraint"),
        keyword_filter: _ => make_keyword("filter"),
        keyword_cast: _ => make_keyword("cast"),
        keyword_separator: _ => make_keyword("separator"),
        keyword_max: _ => make_keyword("max"),
        keyword_min: _ => make_keyword("min"),
        keyword_avg: _ => make_keyword("avg"),
        keyword_case: _ => make_keyword("case"),
        keyword_when: _ => make_keyword("when"),
        keyword_then: _ => make_keyword("then"),
        keyword_else: _ => make_keyword("else"),
        keyword_end: _ => make_keyword("end"),
        keyword_in: _ => make_keyword("in"),
        keyword_and: _ => make_keyword("and"),
        keyword_or: _ => make_keyword("or"),
        keyword_is: _ => make_keyword("is"),
        keyword_not: _ => make_keyword("not"),
        keyword_force: _ => make_keyword("force"),
        keyword_ignore: _ => make_keyword("ignore"),
        keyword_using: _ => make_keyword("using"),
        keyword_use: _ => make_keyword("use"),
        keyword_index: _ => make_keyword("index"),
        keyword_for: _ => make_keyword("for"),
        keyword_if: _ => make_keyword("if"),
        keyword_exists: _ => make_keyword("exists"),
        keyword_auto_increment: _ => make_keyword("auto_increment"),
        keyword_generated: _ => make_keyword("generated"),
        keyword_always: _ => make_keyword("always"),
        keyword_collate: _ => make_keyword("collate"),
        keyword_character: _ => make_keyword("character"),
        keyword_engine: _ => make_keyword("engine"),
        keyword_default: _ => make_keyword("default"),
        keyword_cascade: _ => make_keyword("cascade"),
        keyword_restrict: _ => make_keyword("restrict"),
        keyword_with: _ => make_keyword("with"),
        keyword_no: _ => make_keyword("no"),
        keyword_data: _ => make_keyword("data"),
        keyword_type: _ => make_keyword("type"),
        keyword_rename: _ => make_keyword("rename"),
        keyword_to: _ => make_keyword("to"),
        keyword_database: _ => make_keyword("database"),
        keyword_schema: _ => make_keyword("schema"),
        keyword_owner: _ => make_keyword("owner"),
        keyword_user: _ => make_keyword("user"),
        keyword_admin: _ => make_keyword("admin"),
        keyword_password: _ => make_keyword("password"),
        keyword_encrypted: _ => make_keyword("encrypted"),
        keyword_valid: _ => make_keyword("valid"),
        keyword_until: _ => make_keyword("until"),
        keyword_connection: _ => make_keyword("connection"),
        keyword_role: _ => make_keyword("role"),
        keyword_reset: _ => make_keyword("reset"),
        keyword_temp: _ => make_keyword("temp"),
        keyword_temporary: _ => make_keyword("temporary"),
        keyword_unlogged: _ => make_keyword("unlogged"),
        keyword_logged: _ => make_keyword("logged"),
        keyword_cycle: _ => make_keyword("cycle"),
        keyword_union: _ => make_keyword("union"),
        keyword_all: _ => make_keyword("all"),
        keyword_any: _ => make_keyword("any"),
        keyword_some: _ => make_keyword("some"),
        keyword_except: _ => make_keyword("except"),
        keyword_intersect: _ => make_keyword("intersect"),
        keyword_returning: _ => make_keyword("returning"),
        keyword_begin: _ => make_keyword("begin"),
        keyword_commit: _ => make_keyword("commit"),
        keyword_rollback: _ => make_keyword("rollback"),
        keyword_transaction: _ => make_keyword("transaction"),
        keyword_over: _ => make_keyword("over"),
        keyword_nulls: _ => make_keyword("nulls"),
        keyword_first: _ => make_keyword("first"),
        keyword_after: _ => make_keyword("after"),
        keyword_before: _ => make_keyword("before"),
        keyword_last: _ => make_keyword("last"),
        keyword_window: _ => make_keyword("window"),
        keyword_range: _ => make_keyword("range"),
        keyword_rows: _ => make_keyword("rows"),
        keyword_groups: _ => make_keyword("groups"),
        keyword_between: _ => make_keyword("between"),
        keyword_unbounded: _ => make_keyword("unbounded"),
        keyword_preceding: _ => make_keyword("preceding"),
        keyword_following: _ => make_keyword("following"),
        keyword_exclude: _ => make_keyword("exclude"),
        keyword_current: _ => make_keyword("current"),
        keyword_row: _ => make_keyword("row"),
        keyword_ties: _ => make_keyword("ties"),
        keyword_others: _ => make_keyword("others"),
        keyword_only: _ => make_keyword("only"),
        keyword_unique: _ => make_keyword("unique"),
        keyword_foreign: _ => make_keyword("foreign"),
        keyword_references: _ => make_keyword("references"),
        keyword_concurrently: _ => make_keyword("concurrently"),
        keyword_btree: _ => make_keyword("index"),
        keyword_hash: _ => make_keyword("hash"),
        keyword_gist: _ => make_keyword("gist"),
        keyword_spgist: _ => make_keyword("spgist"),
        keyword_gin: _ => make_keyword("gin"),
        keyword_brin: _ => make_keyword("brin"),
        keyword_like: _ => choice(make_keyword("like"), make_keyword("ilike")),
        keyword_similar: _ => make_keyword("similar"),
        keyword_preserve: _ => make_keyword("preserve"),
        keyword_unsigned: _ => make_keyword("unsigned"),
        keyword_zerofill: _ => make_keyword("zerofill"),
        keyword_conflict: _ => make_keyword("conflict"),
        keyword_do: _ => make_keyword("do"),
        keyword_nothing: _ => make_keyword("nothing"),
        keyword_high_priority: _ => make_keyword("high_priority"),
        keyword_low_priority: _ => make_keyword("low_priority"),
        keyword_delayed: _ => make_keyword("delayed"),
        keyword_recursive: _ => make_keyword("recursive"),
        keyword_cascaded: _ => make_keyword("cascaded"),
        keyword_local: _ => make_keyword("local"),
        keyword_current_timestamp: _ => make_keyword("current_timestamp"),
        keyword_check: _ => make_keyword("check"),
        keyword_option: _ => make_keyword("option"),
        keyword_vacuum: _ => make_keyword("vacuum"),
        keyword_wait: _ => make_keyword("wait"),
        keyword_nowait: _ => make_keyword("nowait"),
        keyword_attribute: _ => make_keyword("attribute"),
        keyword_authorization: _ => make_keyword("authorization"),

        keyword_trigger: _ => make_keyword('trigger'),
        keyword_function: _ => make_keyword("function"),
        keyword_returns: _ => make_keyword("returns"),
        keyword_return: _ => make_keyword("return"),
        keyword_setof: _ => make_keyword("setof"),
        keyword_atomic: _ => make_keyword("atomic"),
        keyword_declare: _ => make_keyword("declare"),
        keyword_language: _ => make_keyword("language"),
        keyword_sql: _ => make_keyword("sql_parser"),
        keyword_plpgsql: _ => make_keyword("plpgsql"),
        keyword_immutable: _ => make_keyword("immutable"),
        keyword_stable: _ => make_keyword("stable"),
        keyword_volatile: _ => make_keyword("volatile"),
        keyword_leakproof: _ => make_keyword("leakproof"),
        keyword_parallel: _ => make_keyword("parallel"),
        keyword_safe: _ => make_keyword("safe"),
        keyword_unsafe: _ => make_keyword("unsafe"),
        keyword_restricted: _ => make_keyword("restricted"),
        keyword_called: _ => make_keyword("called"),
        keyword_returns: _ => make_keyword("returns"),
        keyword_input: _ => make_keyword("input"),
        keyword_strict: _ => make_keyword("strict"),
        keyword_cost: _ => make_keyword("cost"),
        keyword_rows: _ => make_keyword("rows"),
        keyword_support: _ => make_keyword("support"),

        // Hive Keywords
        keyword_external: _ => make_keyword("external"),
        keyword_stored: _ => make_keyword("stored"),
        keyword_cached: _ => make_keyword("cached"),
        keyword_uncached: _ => make_keyword("uncached"),
        keyword_replication: _ => make_keyword("replication"),
        keyword_tblproperties: _ => make_keyword("tblproperties"),
        keyword_options: _ => make_keyword("options"),
        keyword_compute: _ => make_keyword("compute"),
        keyword_stats: _ => make_keyword("stats"),
        keyword_statistics: _ => make_keyword("statistics"),
        keyword_optimize: _ => make_keyword("optimize"),
        keyword_rewrite: _ => make_keyword("rewrite"),
        keyword_bin_pack: _ => make_keyword("bin_pack"),
        keyword_incremental: _ => make_keyword("incremental"),
        keyword_location: _ => make_keyword("location"),
        keyword_partitioned: _ => make_keyword("partitioned"),
        keyword_comment: _ => make_keyword("comment"),
        keyword_sort: _ => make_keyword("sort"),
        keyword_format: _ => make_keyword("format"),
        keyword_delimited: _ => make_keyword("delimited"),
        keyword_fields: _ => make_keyword("fields"),
        keyword_terminated: _ => make_keyword("terminated"),
        keyword_escaped: _ => make_keyword("escaped"),
        keyword_lines: _ => make_keyword("lines"),
        keyword_cache: _ => make_keyword("cache"),
        keyword_metadata: _ => make_keyword("metadata"),
        keyword_noscan: _ => make_keyword("noscan"),

        // Hive file formats
        keyword_parquet: _ => make_keyword("parquet"),
        keyword_rcfile: _ => make_keyword("rcfile"),
        keyword_csv: _ => make_keyword("csv"),
        keyword_textfile: _ => make_keyword("textfile"),
        keyword_avro: _ => make_keyword("avro"),
        keyword_sequencefile: _ => make_keyword("sequencefile"),
        keyword_orc: _ => make_keyword("orc"),
        keyword_avro: _ => make_keyword("avro"),
        keyword_jsonfile: _ => make_keyword("jsonfile"),

        // Operators
        is_not: $ => prec.left(seq($.keyword_is, $.keyword_not)),
        not_like: $ => seq($.keyword_not, $.keyword_like),
        similar_to: $ => seq($.keyword_similar, $.keyword_to),
        not_similar_to: $ => seq($.keyword_not, $.keyword_similar, $.keyword_to),
        distinct_from: $ => seq($.keyword_is, $.keyword_distinct, $.keyword_from),
        not_distinct_from: $ => seq($.keyword_is, $.keyword_not, $.keyword_distinct, $.keyword_from),

        _temporary: $ => choice($.keyword_temp, $.keyword_temporary),
        _not_null: $ => seq($.keyword_not, $.keyword_null),
        _primary_key: $ => seq($.keyword_primary, $.keyword_key),
        _if_exists: $ => seq($.keyword_if, $.keyword_exists),
        _if_not_exists: $ => seq($.keyword_if, $.keyword_not, $.keyword_exists),
        _or_replace: $ => seq($.keyword_or, $.keyword_replace),
        _default_null: $ => seq($.keyword_default, $.keyword_null),
        _current_row: $ => seq($.keyword_current, $.keyword_row),
        _exclude_current_row: $ => seq($.keyword_exclude, $.keyword_current, $.keyword_row),
        _exclude_group: $ => seq($.keyword_exclude, $.keyword_group),
        _exclude_no_others: $ => seq($.keyword_exclude, $.keyword_no, $.keyword_others),
        _exclude_ties: $ => seq($.keyword_exclude, $.keyword_ties),
        _check_option: $ => seq($.keyword_check, $.keyword_option),
        direction: $ => choice($.keyword_desc, $.keyword_asc),

        // Types
        keyword_null: _ => make_keyword("null"),
        keyword_true: _ => make_keyword("true"),
        keyword_false: _ => make_keyword("false"),

        keyword_boolean: _ => make_keyword("boolean"),
        keyword_bit: _ => make_keyword("bit"),
        keyword_binary: _ => make_keyword("binary"),
        keyword_varbinary: _ => make_keyword("varbinary"),
        keyword_image: _ => make_keyword("image"),

        keyword_smallserial: _ => choice(make_keyword("smallserial"), make_keyword("serial2")),
        keyword_serial: _ => choice(make_keyword("serial"), make_keyword("serial4")),
        keyword_bigserial: _ => choice(make_keyword("bigserial"), make_keyword("serial8")),
        keyword_tinyint: _ => choice(make_keyword("tinyint"), make_keyword("int1")),
        keyword_smallint: _ => choice(make_keyword("smallint"), make_keyword("int2")),
        keyword_mediumint: _ => choice(make_keyword("mediumint"), make_keyword("int3")),
        keyword_int: _ => choice(make_keyword("int"), make_keyword("integer"), make_keyword("int4")),
        keyword_bigint: _ => choice(make_keyword("bigint"), make_keyword("int8")),
        keyword_decimal: _ => make_keyword("decimal"),
        keyword_numeric: _ => make_keyword("numeric"),
        keyword_real: _ => choice(make_keyword("real"), make_keyword("float4")),
        keyword_float: _ => make_keyword("float"),
        keyword_double: _ => make_keyword("double"),
        keyword_precision: _ => make_keyword("precision"),
        keyword_inet: _ => make_keyword("inet"),

        keyword_money: _ => make_keyword("money"),
        keyword_smallmoney: _ => make_keyword("smallmoney"),
        keyword_varying: _ => make_keyword("varying"),

        keyword_char: _ => choice(make_keyword("char"), make_keyword("character")),
        keyword_nchar: _ => make_keyword("nchar"),
        keyword_varchar: $ => choice(
            make_keyword("varchar"),
            seq(
                make_keyword("character"),
                $.keyword_varying,
            )
        ),
        keyword_nvarchar: _ => make_keyword("nvarchar"),
        keyword_text: _ => make_keyword("text"),
        keyword_string: _ => make_keyword("string"),
        keyword_uuid: _ => make_keyword("uuid"),

        keyword_json: _ => make_keyword("json"),
        keyword_jsonb: _ => make_keyword("jsonb"),
        keyword_xml: _ => make_keyword("xml"),

        keyword_bytea: _ => make_keyword("bytea"),

        keyword_enum: _ => make_keyword("enum"),

        keyword_date: _ => make_keyword("date"),
        keyword_datetime: _ => make_keyword("datetime"),
        keyword_datetime2: _ => make_keyword("datetime2"),
        keyword_smalldatetime: _ => make_keyword("smalldatetime"),
        keyword_datetimeoffset: _ => make_keyword("datetimeoffset"),
        keyword_time: _ => make_keyword("time"),
        keyword_timestamp: _ => prec.right(
            seq(
                make_keyword("timestamp"),
                optional(
                    seq(
                        make_keyword('without'),
                        make_keyword('time'),
                        make_keyword('zone')
                    ),
                ),
            ),
        ),
        keyword_timestamptz: _ => choice(
            make_keyword('timestamptz'),
            seq(
                make_keyword("timestamp"),
                make_keyword('with'),
                make_keyword('time'),
                make_keyword('zone')
            ),
        ),
        keyword_interval: _ => make_keyword("interval"),

        keyword_geometry: _ => make_keyword("geometry"),
        keyword_geography: _ => make_keyword("geography"),
        keyword_box2d: _ => make_keyword("box2d"),
        keyword_box3d: _ => make_keyword("box3d"),

        keyword_oid: _ => make_keyword("oid"),
        keyword_name: _ => make_keyword("name"),
        keyword_regclass: _ => make_keyword("regclass"),
        keyword_regnamespace: _ => make_keyword("regnamespace"),
        keyword_regproc: _ => make_keyword("regproc"),
        keyword_regtype: _ => make_keyword("regtype"),

        keyword_array: _ => make_keyword("array"), // not included in data_type since it's a constructor literal

        data_type: $ => seq(
            field("data_type_kind", $.data_type_kind),
            optional($.array_size_definition)
        ),

        data_type_kind: $ => choice(
            $.keyword_boolean,
            $.bit,
            $.binary,
            $.varbinary,
            $.keyword_image,

            $.keyword_smallserial,
            $.keyword_serial,
            $.keyword_bigserial,

            $.tinyint,
            $.smallint,
            $.mediumint,
            $.int,
            $.bigint,
            $.decimal,
            $.numeric,
            $.double,
            $.float,
            $.keyword_money,
            $.keyword_smallmoney,
            $.char,
            $.varchar,
            $.nchar,
            $.nvarchar,
            $.numeric,
            $.keyword_string,
            $.keyword_text,

            $.keyword_uuid,

            $.keyword_json,
            $.keyword_jsonb,
            $.keyword_xml,

            $.keyword_bytea,
            $.keyword_inet,

            $.enum,

            $.keyword_date,
            $.keyword_datetime,
            $.keyword_datetime2,
            $.datetimeoffset,
            $.keyword_smalldatetime,
            $.time,
            $.keyword_timestamp,
            $.keyword_timestamptz,
            $.keyword_interval,

            $.keyword_geometry,
            $.keyword_geography,
            $.keyword_box2d,
            $.keyword_box3d,

            $.keyword_oid,
            $.keyword_name,
            $.keyword_regclass,
            $.keyword_regnamespace,
            $.keyword_regproc,
            $.keyword_regtype,

            field("custom_type", $._identifier)
        ),

        array_size_definition: $ => seq(
            choice(
                seq($.keyword_array, optional($._array_size_definition)),
                repeat1($._array_size_definition),
            ),
        ),

        _array_size_definition: $ => seq(
            '[',
            optional(field("size", alias($.integer, $.literal))),
            ']'
        ),

        tinyint: $ => $.keyword_tinyint,
        smallint: $ => $.keyword_smallint,
        mediumint: $ => $.keyword_mediumint,
        int: $ => $.keyword_int,
        bigint: $ => $.keyword_bigint,

        bit: $ => choice(
            $.keyword_bit,
            seq(
                $.keyword_bit,
                prec(0, parametric_type($, $.keyword_varying, ['precision'])),
            ),
            prec(1, parametric_type($, $.keyword_bit, ['precision'])),
        ),

        binary: $ => parametric_type($, $.keyword_binary, ['precision']),
        varbinary: $ => parametric_type($, $.keyword_varbinary, ['precision']),

        // TODO: should qualify against /\\b(0?[1-9]|[1-4][0-9]|5[0-4])\\b/g
        float: $ => choice(
            parametric_type($, $.keyword_float, ['precision']),
            unsigned_type($, parametric_type($, $.keyword_float, ['precision', 'scale'])),
        ),

        double: $ => choice(
            make_keyword("float8"),
            unsigned_type($, parametric_type($, $.keyword_double, ['precision', 'scale'])),
            unsigned_type($, parametric_type($, seq($.keyword_double, $.keyword_precision), ['precision', 'scale'])),
            unsigned_type($, parametric_type($, $.keyword_real, ['precision', 'scale'])),
        ),

        decimal: $ => choice(
            parametric_type($, $.keyword_decimal, ['precision']),
            parametric_type($, $.keyword_decimal, ['precision', 'scale']),
        ),
        numeric: $ => choice(
            parametric_type($, $.keyword_numeric, ['precision']),
            parametric_type($, $.keyword_numeric, ['precision', 'scale']),
        ),
        char: $ => seq($.keyword_char, '(', field("length", $.natural_number), ')'),
        varchar: $ => seq($.keyword_varchar, '(', field("length", $.natural_number), ')'),
        nchar: $ => parametric_type($, $.keyword_nchar),
        nvarchar: $ => parametric_type($, $.keyword_nvarchar),

        datetimeoffset: $ => parametric_type($, $.keyword_datetimeoffset),
        time: $ => parametric_type($, $.keyword_time),

        enum: $ => seq(
            $.keyword_enum,
            paren_list(field("value", alias($.literal_string, $.literal)), true)
        ),

        array: $ => seq(
            $.keyword_array,
            choice(
                seq(
                    "[",
                    comma_list($.expression),
                    "]"
                ),
                seq(
                    "(",
                    $.dml_read_stmt,
                    ")",
                )
            )
        ),

        comment: _ => seq('--', /.*/),
        // https://stackoverflow.com/questions/13014947/regex-to-match-a-c-style-multiline-comment
        marginalia: _ => seq('/*', /[^*]*\*+(?:[^/*][^*]*\*+)*/, '/'),

        statement_transaction: $ => choice(
            field("begin_transaction", $.begin_transaction),
            field("commit_transaction", $.commit_transaction),
            field("rollback_transaction", $.rollback_transaction),
            field("statement", $.statement)
        ),

        begin_transaction: $ => seq(
            $.keyword_begin,
            $.keyword_transaction
        ),

        commit_transaction: $ => seq(
            $.keyword_commit,
            optional(
                $.keyword_transaction,
            ),
        ),

        rollback_transaction: $ => seq(
            $.keyword_rollback,
            optional(
                $.keyword_transaction,
            ),
        ),

        statement: $ => seq(
            optional(seq(
                $.keyword_explain,
                optional($.keyword_analyze),
                optional($.keyword_verbose),
            )),
            field("stmt_gut",
                choice(
                    $.ddl_stmt,
                    $.dml_write_stmt,
                    $.dml_read_stmt,
                    $.copy_stmt,
                )
            ),
        ),

        copy_stmt: $ => choice(
            $.copy_from,
            $.copy_to,
        ),

        copy_from: $ => seq(
            $.keyword_copy,
            field('object_reference', $.object_reference),
            $.keyword_from,
            field('file_path', $.file_path),
        ),

        copy_to: $ => seq(
            $.keyword_copy,
            $.object_reference,
            $.keyword_to,
            $.file_path,
        ),
        file_path: $ =>
            $.literal_string,

        ddl_stmt: $ => choice(
            $.create_table_statement,
            $._alter_statement,
            $.drop_statement,
            $._rename_statement,
            $._optimize_statement,
            $._merge_statement,
        ),

        _cte: $ => seq(
            $.keyword_with,
            optional($.keyword_recursive),
            $.cte,
            repeat(
                seq(
                    ',',
                    $.cte,
                ),
            ),
        ),

        dml_write_stmt: $ => choice(
            $.delete_statement,
            $.insert_statement,
            $.update_statement,
            $._truncate_statement,
        ),


        dml_read_stmt: $ => $.select_statement,

        cte: $ => seq(
            $.identifier,
            optional(paren_list(field("argument", $.identifier))),
            $.keyword_as,
            optional(
                seq(
                    optional($.keyword_not),
                    $.keyword_materialized,
                ),
            ),
            wrapped_in_parenthesis(
                alias(
                    choice($.dml_read_stmt, $.dml_write_stmt),
                    $.statement,
                ),
            ),
        ),

        set_operation: $ => seq(
            $.select_statement,
            repeat1(
                seq(
                    field(
                        "operation",
                        choice(
                            seq($.keyword_union, optional($.keyword_all)),
                            $.keyword_except,
                            $.keyword_intersect,
                        ),
                    ),
                    $.select_statement,
                ),
            ),
        ),

        select_statement: $ =>
            seq(
                field("select", $.select),
                field("from", $.from),
            ),


        select: $ => seq(
            $.keyword_select,
            seq(
                optional(field("distinct", $.keyword_distinct)),
                field("select_expression", $.select_expression),
            ),
        ),

        select_expression: $ => seq(
            $.term,
            repeat(
                seq(
                    ',',
                    $.term,
                ),
            ),
        ),

        term: $ => choice(
            field("all_fields", $.all_fields),
            seq(field("expression", $.expression), optional(field("alias", $.alias_name))),
        ),

        _truncate_statement: $ => seq(
            $.keyword_truncate,
            optional($.keyword_table),
            optional($.keyword_only),
            comma_list($.object_reference),
            optional($._drop_behavior),
        ),

        delete_statement: $ => seq(
            $.keyword_delete,
            $.keyword_from,
            field('object_reference', $.object_reference),
            field('where', $.where),
        ),

        delete: $ => seq(
            $.keyword_delete,
            optional($.index_hint),
        ),

        _create_statement: $ => seq(
            choice(
                $.create_table_statement,
                $.create_view,
                $.create_materialized_view,
                $.create_index,
                $.create_function,
                $.create_type,
                $.create_database,
                $.create_role,
                $.create_sequence,
                prec.left(seq(
                    $.create_schema,
                    repeat($._create_statement),
                )),
            ),
        ),

        _table_settings: $ => choice(
            $.table_partition,
            $.stored_as,
            $.storage_location,
            $.table_sort,
            $.row_format,
            seq(
                $.keyword_tblproperties,
                paren_list($.table_option, true),
            ),
            $.table_option,
        ),

        // left precedence because 'quoted' table options otherwise conflict with
        // `create function` string bodies; if you remove this precedence you will
        // have to also disable the `literal_string` choice for the `name` field
        // in =-assigned `table_option`s
        create_table_statement: $ => prec.left(
            seq(
                $.keyword_create,
                optional(
                    choice(
                        $._temporary,
                        $.keyword_unlogged,
                        $.keyword_external,
                    )
                ),
                $.keyword_table,
                optional(field("if_not_exists", $._if_not_exists)),
                field("table_name", $.identifier),
                field("column_definitions", $.column_definitions),
            ),
        ),

        create_query: $ => $.dml_read_stmt,

        create_view: $ => prec.right(
            seq(
                $.keyword_create,
                optional($._or_replace),
                optional($._temporary),
                optional($.keyword_recursive),
                $.keyword_view,
                optional($._if_not_exists),
                $.object_reference,
                optional(paren_list($.identifier)),
                $.keyword_as,
                $.create_query,
                optional(
                    seq(
                        $.keyword_with,
                        optional(
                            choice(
                                $.keyword_local,
                                $.keyword_cascaded,
                            )
                        ),
                        $._check_option,
                    ),
                ),
            ),
        ),

        create_materialized_view: $ => prec.right(
            seq(
                $.keyword_create,
                optional($._or_replace),
                $.keyword_materialized,
                $.keyword_view,
                optional($._if_not_exists),
                $.object_reference,
                $.keyword_as,
                $.create_query,
                optional(
                    choice(
                        seq(
                            $.keyword_with,
                            $.keyword_data,
                        ),
                        seq(
                            $.keyword_with,
                            $.keyword_no,
                            $.keyword_data,
                        )
                    )
                )
            ),
        ),

        // TODO arbitrary dollar quoting, mostly ensuring that the initial and terminal quote
        // delimiters match, will require an external scanner
        // https://tree-sitter.github.io/tree-sitter/creating-parsers#external-scanners
        dollar_quote: () => choice(
            '$$',
            '$function$',
            '$body$'
        ),

        create_function: $ => seq(
            $.keyword_create,
            optional($._or_replace),
            $.keyword_function,
            $.object_reference,
            choice(
                $.column_definitions, // TODO `default` will require own node type
                wrapped_in_parenthesis(),
            ),
            $.keyword_returns,
            choice(
                $.data_type,
                seq($.keyword_setof, $.data_type),
                seq($.keyword_table, $.column_definitions),
                $.keyword_trigger,
            ),
            repeat(
                choice(
                    $.function_language,
                    $.function_volatility,
                    $.function_leakproof,
                    $.function_safety,
                    $.function_strictness,
                    $.function_cost,
                    $.function_rows,
                    $.function_support,
                ),
            ),
            // ensure that there's only one function body -- other specifiers are less
            // variable but the body can have all manner of conflicting stuff
            $.function_body,
            repeat(
                choice(
                    $.function_language,
                    $.function_volatility,
                    $.function_leakproof,
                    $.function_safety,
                    $.function_strictness,
                    $.function_cost,
                    $.function_rows,
                    $.function_support,
                ),
            ),
        ),

        _function_return: $ => seq(
            $.keyword_return,
            $.expression,
        ),

        function_declaration: $ => seq(
            $.identifier,
            $.data_type,
            optional(
                seq(
                    ':=',
                    choice(
                        wrapped_in_parenthesis($.statement),
                        // TODO are there more possibilities here? We can't use `expression` since
                        // that includes subqueries
                        $.literal,
                    ),
                ),
            ),
            ';',
        ),

        _function_body_statement: $ => choice(
            $.statement,
            $._function_return,
        ),

        function_body: $ => choice(
            seq(
                $._function_return,
                ';'
            ),
            seq(
                $.keyword_begin,
                $.keyword_atomic,
                repeat1(
                    seq(
                        $._function_body_statement,
                        ';',
                    ),
                ),
                $.keyword_end,
            ),
            seq(
                $.keyword_as,
                $.dollar_quote,
                optional(
                    seq(
                        $.keyword_declare,
                        repeat1(
                            $.function_declaration,
                        ),
                    ),
                ),
                $.keyword_begin,
                repeat1(
                    seq(
                        $._function_body_statement,
                        ';',
                    ),
                ),
                $.keyword_end,
                optional(';'),
                $.dollar_quote,
            ),
            seq(
                $.keyword_as,
                alias($.literal_string, $.literal),
            ),
            seq(
                $.keyword_as,
                $.dollar_quote,
                $._function_body_statement,
                $.dollar_quote,
            ),
        ),

        function_language: $ => seq(
            $.keyword_language,
            choice(
                $.keyword_sql,
                $.keyword_plpgsql,
            ),
        ),

        function_volatility: $ => choice(
            $.keyword_immutable,
            $.keyword_stable,
            $.keyword_volatile,
        ),

        function_leakproof: $ => seq(
            optional($.keyword_not),
            $.keyword_leakproof,
        ),

        function_safety: $ => seq(
            $.keyword_parallel,
            choice(
                $.keyword_safe,
                $.keyword_unsafe,
                $.keyword_restricted,
            ),
        ),

        function_strictness: $ => choice(
            seq(
                choice(
                    $.keyword_called,
                    seq(
                        $.keyword_returns,
                        $.keyword_null,
                    ),
                ),
                $.keyword_on,
                $.keyword_null,
                $.keyword_input,
            ),
            $.keyword_strict,
        ),

        function_cost: $ => seq(
            $.keyword_cost,
            $.natural_number,
        ),

        function_rows: $ => seq(
            $.keyword_rows,
            $.natural_number,
        ),

        function_support: $ => seq(
            $.keyword_support,
            alias($.literal_string, $.literal),
        ),

        create_index: $ => seq(
            $.keyword_create,
            optional($.keyword_unique),
            $.keyword_index,
            optional($.keyword_concurrently),
            optional(
                seq(
                    optional($._if_not_exists),
                    field("column", $.column),
                ),
            ),
            $.keyword_on,
            optional($.keyword_only),
            seq(
                $.object_reference,
                optional(
                    seq(
                        $.keyword_using,
                        choice(
                            $.keyword_btree,
                            $.keyword_hash,
                            $.keyword_gist,
                            $.keyword_spgist,
                            $.keyword_gin,
                            $.keyword_brin
                        ),
                    ),
                ),
                $.ordered_columns,
            ),
            optional(
                $.where,
            ),
        ),

        create_schema: $ => prec.left(seq(
            $.keyword_create,
            $.keyword_schema,
            choice(
                seq(
                    optional($._if_not_exists),
                    $.identifier,
                    optional(seq($.keyword_authorization, $.identifier)),
                ),
                seq(
                    $.keyword_authorization,
                    $.identifier,
                ),
            ),
        )),

        _with_settings: $ => seq(
            field('name', $.identifier),
            optional('='),
            field('value', choice($.identifier, alias($._single_quote_string, $.literal))),
        ),

        create_database: $ => seq(
            $.keyword_create,
            $.keyword_database,
            $.identifier,
            optional($.keyword_with),
            repeat(
                $._with_settings
            ),
        ),

        create_role: $ => seq(
            $.keyword_create,
            choice(
                $.keyword_user,
                $.keyword_role,
                $.keyword_group,
            ),
            $.identifier,
            optional($.keyword_with),
            repeat(
                choice(
                    $._user_access_role_config,
                    $._role_options,
                ),
            ),
        ),

        _role_options: $ => choice(
            field("option", $.identifier),
            seq(
                $.keyword_valid,
                $.keyword_until,
                field("valid_until", alias($.literal_string, $.literal))
            ),
            seq(
                $.keyword_connection,
                $.keyword_limit,
                field("connection_limit", alias($.integer, $.literal))
            ),
            seq(
                optional($.keyword_encrypted),
                $.keyword_password,
                choice(
                    field("password", alias($.literal_string, $.literal)),
                    $.keyword_null,
                ),
            ),
        ),

        _user_access_role_config: $ => seq(
            choice(
                seq(optional($.keyword_in), $.keyword_role),
                seq($.keyword_in, $.keyword_group),
                $.keyword_admin,
                $.keyword_user,
            ),
            comma_list($.identifier, true),
        ),

        create_sequence: $ => seq(
            $.keyword_create,
            optional(
                choice(
                    choice($.keyword_temporary, $.keyword_temp),
                    $.keyword_unlogged,
                )
            ),
            $.keyword_sequence,
            optional($._if_not_exists),
            $.object_reference,
            repeat(
                choice(
                    seq($.keyword_as, $.data_type),
                    seq($.keyword_increment, optional($.keyword_by), field("increment", alias($.integer, $.literal))),
                    seq($.keyword_minvalue, choice($.literal, seq($.keyword_no, $.keyword_minvalue))),
                    seq($.keyword_maxvalue, choice($.literal, seq($.keyword_no, $.keyword_maxvalue))),
                    seq($.keyword_start, optional($.keyword_with), field("start", alias($.integer, $.literal))),
                    seq($.keyword_cache, field("cache", alias($.integer, $.literal))),
                    seq(optional($.keyword_no), $.keyword_cycle),
                    seq($.keyword_owned, $.keyword_by, choice($.keyword_none, $.object_reference)),
                )
            ),
        ),

        create_type: $ => seq(
            $.keyword_create,
            $.keyword_type,
            $.identifier,
            optional(
                seq(
                    choice(
                        seq(
                            $.keyword_as,
                            $.column_definitions,
                            optional(seq($.keyword_collate, $.identifier))
                        ),
                        seq(
                            $.keyword_as,
                            $.keyword_enum,
                            $.enum_elements,
                        ),
                        seq(
                            optional(
                                seq(
                                    $.keyword_as,
                                    $.keyword_range,
                                )
                            ),
                            paren_list(
                                $._with_settings
                            ),
                        ),
                    ),
                ),
            ),
        ),

        enum_elements: $ => seq(
            paren_list(field("enum_element", alias($.literal_string, $.literal))),
        ),

        _alter_statement: $ => seq(
            choice(
                $.alter_table,
                $.alter_view,
                $.alter_schema,
                $.alter_type,
                $.alter_index,
                $.alter_database,
                $.alter_role,
                $.alter_sequence,
            ),
        ),

        _rename_statement: $ => seq(
            $.keyword_rename,
            choice(
                $.keyword_table,
                $.keyword_tables,
            ),
            optional($._if_exists),
            $.object_reference,
            optional(
                choice(
                    $.keyword_nowait,
                    seq(
                        $.keyword_wait,
                        field('timeout', alias($.natural_number, $.literal))
                    )
                )
            ),
            $.keyword_to,
            $.object_reference,
            repeat(
                seq(
                    ',',
                    $._rename_table_names,
                )
            ),
        ),

        _rename_table_names: $ => seq(
            $.object_reference,
            $.keyword_to,
            $.object_reference,
        ),

        alter_table: $ => seq(
            $.keyword_alter,
            $.keyword_table,
            optional($._if_exists),
            $.object_reference,
            choice(
                seq(
                    $._alter_specifications,
                    repeat(
                        seq(
                            ",",
                            $._alter_specifications
                        )
                    )
                ),
            ),
        ),

        _alter_specifications: $ => choice(
            $.add_column,
            $.add_constraint,
            $.alter_column,
            $.modify_column,
            $.change_column,
            $.drop_column,
            $.rename_object,
            $.rename_column,
            $.set_schema,
            $.change_ownership,
        ),

        // TODO: optional `keyword_add` is necessary to allow for chained alter statements in t-sql_parser
        // maybe needs refactoring
        add_column: $ => seq(
            optional($.keyword_add),
            optional(
                $.keyword_column,
            ),
            optional($._if_not_exists),
            $.column_definition,
            optional($.column_position),
        ),

        add_constraint: $ => seq(
            $.keyword_add,
            optional($.keyword_constraint),
            $.identifier,
            $.constraint,
        ),

        alter_column: $ => seq(
            // TODO constraint management
            $.keyword_alter,
            optional(
                $.keyword_column,
            ),
            field('name', $.identifier),
            choice(
                seq(
                    choice(
                        $.keyword_set,
                        $.keyword_drop,
                    ),
                    $.keyword_not,
                    $.keyword_null,
                ),
                seq(
                    optional(
                        seq(
                            $.keyword_set,
                            $.keyword_data,
                        ),
                    ),
                    $.keyword_type,
                    field('type', $.data_type),
                ),
                seq(
                    $.keyword_set,
                    $.keyword_default,
                    $.expression,
                ),
                seq(
                    $.keyword_drop,
                    $.keyword_default,
                ),
            ),
        ),

        modify_column: $ => seq(
            $.keyword_modify,
            optional(
                $.keyword_column,
            ),
            optional($._if_exists),
            $.column_definition,
            optional($.column_position),
        ),

        change_column: $ => seq(
            $.keyword_change,
            optional(
                $.keyword_column,
            ),
            optional($._if_exists),
            field('old_name', $.identifier),
            $.column_definition,
            optional($.column_position),
        ),

        column_position: $ => choice(
            $.keyword_first,
            seq(
                $.keyword_after,
                field('col_name', $.identifier),
            ),
        ),

        drop_column: $ => seq(
            $.keyword_drop,
            optional(
                $.keyword_column,
            ),
            optional($._if_exists),
            field('name', $.identifier),
        ),

        rename_column: $ => seq(
            $.keyword_rename,
            optional(
                $.keyword_column,
            ),
            field('old_name', $.identifier),
            $.keyword_to,
            field('new_name', $.identifier),
        ),

        alter_view: $ => seq(
            $.keyword_alter,
            $.keyword_view,
            optional($._if_exists),
            $.object_reference,
            choice(
                // TODO Postgres allows a single "alter column" to set or drop default
                $.rename_object,
                $.rename_column,
                $.set_schema,
                $.change_ownership,
            ),
        ),

        alter_schema: $ => seq(
            $.keyword_alter,
            $.keyword_schema,
            $.identifier,
            choice(
                $.keyword_rename,
                $.keyword_owner,
            ),
            $.keyword_to,
            $.identifier,
        ),

        alter_database: $ => seq(
            $.keyword_alter,
            $.keyword_database,
            $.identifier,
            optional($.keyword_with),
            choice(
                seq($.rename_object),
                seq($.change_ownership),
                seq(
                    $.keyword_reset,
                    choice(
                        $.keyword_all,
                        field("configuration_parameter", $.identifier)
                    ),
                ),
                seq(
                    $.keyword_set,
                    choice(
                        seq($.keyword_tablespace, $.identifier),
                        $.set_configuration,
                    ),
                ),
            ),
        ),

        alter_role: $ => seq(
            $.keyword_alter,
            choice(
                $.keyword_role,
                $.keyword_group,
                $.keyword_user,
            ),
            choice($.identifier, $.keyword_all),
            choice(
                $.rename_object,
                seq(optional($.keyword_with), repeat($._role_options)),
                seq(
                    optional(seq($.keyword_in, $.keyword_database, $.identifier)),
                    choice(
                        seq(
                            $.keyword_set,
                            $.set_configuration,
                        ),
                        seq(
                            $.keyword_reset,
                            choice(
                                $.keyword_all,
                                field("option", $.identifier),
                            )),
                    ),
                )
            ),
        ),

        set_configuration: $ => seq(
            field("option", $.identifier),
            choice(
                seq($.keyword_from, $.keyword_current),
                seq(
                    choice($.keyword_to, "="),
                    choice(
                        field("parameter", $.identifier),
                        $.literal,
                        $.keyword_default
                    )
                )
            ),
        ),

        alter_index: $ => seq(
            $.keyword_alter,
            $.keyword_index,
            optional($._if_exists),
            $.identifier,
            choice(
                $.rename_object,
                seq(
                    $.keyword_alter,
                    optional($.keyword_column),
                    alias($.natural_number, $.literal),
                    $.keyword_set,
                    $.keyword_statistics,
                    alias($.natural_number, $.literal),
                ),
                seq($.keyword_reset, paren_list($.identifier)),
                seq(
                    $.keyword_set,
                    choice(
                        seq($.keyword_tablespace, $.identifier),
                        paren_list(seq($.identifier, '=', field("value", $.literal)))
                    ),
                ),
            ),
        ),

        alter_sequence: $ => seq(
            $.keyword_alter,
            $.keyword_sequence,
            optional($._if_exists),
            $.object_reference,
            choice(
                repeat1(
                    choice(
                        seq($.keyword_as, $.data_type),
                        seq($.keyword_increment, optional($.keyword_by), $.literal),
                        seq($.keyword_minvalue, choice($.literal, seq($.keyword_no, $.keyword_minvalue))),
                        seq($.keyword_maxvalue, choice($.literal, seq($.keyword_no, $.keyword_maxvalue))),
                        seq($.keyword_start, optional($.keyword_with), field("start", alias($.integer, $.literal))),
                        seq($.keyword_restart, optional($.keyword_with), field("restart", alias($.integer, $.literal))),
                        seq($.keyword_cache, field("cache", alias($.integer, $.literal))),
                        seq(optional($.keyword_no), $.keyword_cycle),
                        seq($.keyword_owned, $.keyword_by, choice($.keyword_none, $.object_reference)),
                    ),
                ),
                $.rename_object,
                $.change_ownership,
                seq(
                    $.keyword_set,
                    choice(
                        choice($.keyword_logged, $.keyword_unlogged),
                        seq($.keyword_schema, $.identifier)
                    ),
                ),
            ),
        ),

        alter_type: $ => seq(
            $.keyword_alter,
            $.keyword_type,
            $.identifier,
            choice(
                $.change_ownership,
                $.set_schema,
                $.rename_object,
                seq(
                    $.keyword_rename,
                    $.keyword_attribute,
                    $.identifier,
                    $.keyword_to,
                    $.identifier,
                    optional($._drop_behavior)
                ),
                seq(
                    $.keyword_add,
                    $.keyword_value,
                    optional($._if_not_exists),
                    alias($._single_quote_string, $.literal),
                    optional(
                        seq(
                            choice($.keyword_before, $.keyword_after),
                            alias($._single_quote_string, $.literal),
                        )
                    ),
                ),
                seq(
                    $.keyword_rename,
                    $.keyword_value,
                    alias($._single_quote_string, $.literal),
                    $.keyword_to,
                    alias($._single_quote_string, $.literal),
                ),
                seq(
                    choice(
                        seq(
                            $.keyword_add,
                            $.keyword_attribute,
                            $.identifier,
                            $.data_type
                        ),
                        seq($.keyword_drop,
                            $.keyword_attribute,
                            optional($._if_exists),
                            $.identifier),
                        seq(
                            $.keyword_alter,
                            $.keyword_attribute,
                            $.identifier,
                            optional(seq($.keyword_set, $.keyword_data)),
                            $.keyword_type,
                            $.data_type
                        ),
                    ),
                    optional(seq($.keyword_collate, $.identifier)),
                    optional($._drop_behavior)
                )
            ),
        ),

        _drop_behavior: $ => choice(
            $.keyword_cascade,
            $.keyword_restrict,
        ),

        drop_statement: $ => choice(
            $.drop_table,
            $.drop_view,
            $.drop_index,
            $.drop_type,
            $.drop_schema,
            $.drop_database,
            $.drop_role,
            $.drop_sequence,
        ),

        drop_table: $ => seq(
            $.keyword_drop,
            $.keyword_table,
            optional(field("if_exist", $._if_exists)),
            field("object_reference", $.object_reference),
            optional($._drop_behavior),
        ),

        drop_view: $ => seq(
            $.keyword_drop,
            $.keyword_view,
            optional($._if_exists),
            $.object_reference,
            optional($._drop_behavior),
        ),

        drop_schema: $ => seq(
            $.keyword_drop,
            $.keyword_schema,
            optional($._if_exists),
            $.identifier,
            optional($._drop_behavior)
        ),

        drop_database: $ => seq(
            $.keyword_drop,
            $.keyword_database,
            optional($._if_exists),
            $.identifier,
            optional($.keyword_with),
            optional($.keyword_force),
        ),

        drop_role: $ => seq(
            $.keyword_drop,
            choice(
                $.keyword_group,
                $.keyword_role,
                $.keyword_user,
            ),
            optional($._if_exists),
            $.identifier,
        ),

        drop_type: $ => seq(
            $.keyword_drop,
            $.keyword_type,
            optional($._if_exists),
            $.object_reference,
            optional($._drop_behavior),
        ),

        drop_sequence: $ => seq(
            $.keyword_drop,
            $.keyword_sequence,
            optional($._if_exists),
            $.object_reference,
            optional($._drop_behavior),
        ),

        drop_index: $ => seq(
            $.keyword_drop,
            $.keyword_index,
            optional($.keyword_concurrently),
            optional($._if_exists),
            field('identifier_name', $.identifier),
            optional($._drop_behavior),
            optional(
                seq(
                    $.keyword_on,
                    $.object_reference,
                ),
            ),
        ),

        rename_object: $ => seq(
            $.keyword_rename,
            $.keyword_to,
            $.object_reference,
        ),

        set_schema: $ => seq(
            $.keyword_set,
            $.keyword_schema,
            field('schema', $.identifier),
        ),

        change_ownership: $ => seq(
            $.keyword_owner,
            $.keyword_to,
            $.identifier,
        ),

        object_reference: $ => seq(
            optional(
                seq(
                    field('schema_name', $.identifier),
                    '.',
                ),
            ),
            field('object_name', $.identifier),
        ),

        insert_statement: $ => seq(
            $.keyword_insert,
            $.keyword_into,
            field('object_reference', $.object_reference),
            field('insert_values', $.insert_values),
        ),

        insert_values: $ => seq(
            optional(field('column_list', $.column_list)),
            seq(
                $.keyword_values,
                field('typed_row_value_expr_list', $.typed_row_value_expr_list),
            ),
        ),

        typed_row_value_expr_list: $ =>
            comma_list(field('list', $.list), true),

        set_values: $ => comma_list(field('assignment', $.assignment), true),

        column_list: $ => paren_list(field('column', $.column), true),

        column: $ => $.identifier,

        update_statement: $ => seq(
            $.keyword_update,
            field('object_reference', $.object_reference),
            $.keyword_set,
            field('set_values', $.set_values),
            optional(field('where', $.where)),
        ),


        _merge_statement: $ => seq(
            $.keyword_merge,
            $.keyword_into,
            $.object_reference,
            optional($.alias_name),
            $.keyword_using,
            choice(
                $.subquery,
                $.object_reference
            ),
            optional($.alias_name),
            $.keyword_on,
            optional_parenthesis(field('predicate', $.expression)),
            repeat1($.when_clause)
        ),

        when_clause: $ => seq(
            $.keyword_when,
            optional($.keyword_not),
            $.keyword_matched,
            optional(
                seq(
                    $.keyword_and,
                    optional_parenthesis(field('predicate', $.expression))
                )
            ),
            $.keyword_then,
            choice(
                $.keyword_delete,
                seq(
                    $.keyword_update,
                    $.set_values,
                ),
                seq(
                    $.keyword_insert,
                    $.insert_values
                ),
                optional($.where)
            )
        ),

        _optimize_statement: $ => choice(
            $._compute_stats,
            $._vacuum_table,
            $._optimize_table,
        ),

        // Compute stats for Impala and Hive
        _compute_stats: $ => choice(
            // Hive
            seq(
                $.keyword_analyze,
                $.keyword_table,
                $.object_reference,
                optional($._partition_spec),
                $.keyword_compute,
                $.keyword_statistics,
                optional(
                    seq(
                        $.keyword_for,
                        $.keyword_columns
                    )
                ),
                optional(
                    seq(
                        $.keyword_cache,
                        $.keyword_metadata
                    )
                ),
                optional($.keyword_noscan),
            ),
            // Impala
            seq(
                $.keyword_compute,
                optional(
                    $.keyword_incremental,
                ),
                $.keyword_stats,
                $.object_reference,
                optional(
                    choice(
                        paren_list(repeat1($.field)),
                        $._partition_spec,
                    )
                )
            ),
        ),

        _optimize_table: $ => choice(
            // Athena/Iceberg
            seq(
                $.keyword_optimize,
                $.object_reference,
                $.keyword_rewrite,
                $.keyword_data,
                $.keyword_using,
                $.keyword_bin_pack,
                optional(
                    $.where,
                )
            ),
            // MariaDB Optimize
            seq(
                $.keyword_optimize,
                optional(
                    choice(
                        $.keyword_local,
                        //$.keyword_no_write_to_binlog,
                    )
                ),
                $.keyword_table,
                $.object_reference,
                repeat(seq(',', $.object_reference)),
            ),
        ),

        _vacuum_table: $ => seq(
            $.keyword_vacuum,
            optional($._vacuum_option),
            $.object_reference,
            optional(
                paren_list($.field)
            ),
        ),

        _vacuum_option: $ => choice(
            seq($.keyword_full, optional(choice($.keyword_true, $.keyword_false))),
            seq($.keyword_parallel, optional(choice($.keyword_true, $.keyword_false))),
            seq($.keyword_analyze, optional(choice($.keyword_true, $.keyword_false))),
            // seq($.keyword_freeze, choice($.keyword_true, $.keyword_false)),
            // seq($.keyword_skip_locked, choice($.keyword_true, $.keyword_false)),
            // seq($.keyword_truncate, choice($.keyword_true, $.keyword_false)),
            // seq($.keyword_disable_page_skipping, choice($.keyword_true, $.keyword_false)),
            // seq($.keyword_process_toast, choice($.keyword_true, $.keyword_false)),
            // seq($.keyword_index_cleanup, choice($.keyword_auto, $.keyword_on, $.keyword_off)),
        ),

        // TODO: this does not account for partitions specs like
        // (partcol1='2022-01-01', hr=11)
        // the second argument is not a $.table_option
        _partition_spec: $ => seq(
            $.keyword_partition,
            paren_list($.table_option, true),
        ),

        _mysql_update_statement: $ => prec(0,
            seq(
                comma_list($.relation, true),
                repeat($.join),
                $.set_values,
                optional($.where),
            ),
        ),

        _postgres_update_statement: $ => prec(1,
            seq(
                $.relation,
                $.set_values,
                optional($.from),
            ),
        ),

        storage_location: $ => prec.right(
            seq(
                $.keyword_location,
                field('path', alias($.literal_string, $.literal)),
                optional(
                    seq(
                        $.keyword_cached,
                        $.keyword_in,
                        field('pool', alias($.literal_string, $.literal)),
                        optional(
                            choice(
                                $.keyword_uncached,
                                seq(
                                    $.keyword_with,
                                    $.keyword_replication,
                                    '=',
                                    field('value', alias($.natural_number, $.literal)),
                                ),
                            ),
                        ),
                    )
                )
            ),
        ),

        row_format: $ => seq(
            $.keyword_row,
            $.keyword_format,
            $.keyword_delimited,
            optional(
                seq(
                    $.keyword_fields,
                    $.keyword_terminated,
                    $.keyword_by,
                    field('fields_terminated_char', alias($.literal_string, $.literal)),
                    optional(
                        seq(
                            $.keyword_escaped,
                            $.keyword_by,
                            field('escaped_char', alias($.literal_string, $.literal)),
                        )
                    )
                )
            ),
            optional(
                seq(
                    $.keyword_lines,
                    $.keyword_terminated,
                    $.keyword_by,
                    field('row_terminated_char', alias($.literal_string, $.literal)),
                )
            )
        ),

        table_sort: $ => seq(
            $.keyword_sort,
            $.keyword_by,
            paren_list($.identifier, true),
        ),

        table_partition: $ => seq(
            choice(
                // Postgres/MySQL style
                seq(
                    $.keyword_partition,
                    $.keyword_by,
                    choice(
                        $.keyword_range,
                        $.keyword_hash,
                    )
                ),
                // Hive style
                seq(
                    $.keyword_partitioned,
                    $.keyword_by,
                ),
                // Spark SQL
                $.keyword_partition,
            ),
            choice(
                paren_list($.identifier),// postgres & Impala (CTAS)
                $.column_definitions, // impala/hive external tables
                paren_list($._key_value_pair, true), // Spark SQL
            )
        ),

        _key_value_pair: $ => seq(
            field('key', $.identifier),
            '=',
            field('value', alias($.literal_string, $.literal)),
        ),

        stored_as: $ => seq(
            $.keyword_stored,
            $.keyword_as,
            choice(
                $.keyword_parquet,
                $.keyword_csv,
                $.keyword_sequencefile,
                $.keyword_textfile,
                $.keyword_rcfile,
                $.keyword_orc,
                $.keyword_avro,
                $.keyword_jsonfile,
            ),
        ),

        assignment: $ => seq(
            field('left', $.field),
            '=',
            field('right', $.expression),
        ),

        table_option: $ => choice(
            seq($.keyword_default, $.keyword_character, $.keyword_set, $.identifier),
            seq($.keyword_collate, $.identifier),
            field('name', $.keyword_default),
            seq(
                field('name', choice($.keyword_engine, $.identifier, $.literal_string)),
                '=',
                field('value', choice($.identifier, $.literal_string)),
            ),
        ),

        column_definitions: $ => seq(
            '(',
            comma_list($.column_definition, true),
            optional($.constraints),
            ')',
        ),

        column_definition: $ => seq(
            field('column_name', $.identifier),
            field('data_type', $.data_type),
            repeat(field('column_constraint', $.column_constraint)),
        ),

        _column_comment: $ => seq(
            $.keyword_comment,
            alias($.literal_string, $.literal)
        ),

        column_constraint: $ => choice(
            choice(
                $.keyword_null,
                $._not_null,
            ),
            $._default_expression,
            field('primary_key', $._primary_key),
            $.keyword_auto_increment,
            $.direction,
            $._column_comment,
            seq(
                optional(seq($.keyword_generated, $.keyword_always)),
                $.keyword_as,
                $.identifier,
            ),
        ),

        _default_expression: $ => seq(
            $.keyword_default,
            optional_parenthesis($._inner_default_expression),
        ),
        _inner_default_expression: $ => choice(
            $.literal,
            $.list,
            $.cast,
            $.binary_expression,
            $.unary_expression,
            $.array,
            $.invocation,
            $.keyword_current_timestamp,
            alias($.implicit_cast, $.cast),
        ),

        constraints: $ => seq(
            ',',
            field('constraint', $.constraint),
            repeat(
                seq(',', field('constraint', $.constraint)),
            ),
        ),

        constraint: $ => choice(
            $._constraint_literal,
            $._key_constraint,
            field('primary_key_constraint', $.primary_key_constraint),
        ),

        _constraint_literal: $ => seq(
            $.keyword_constraint,
            field('name', $.identifier),
            $._primary_key,
            $.column_list,
        ),

        primary_key_constraint: $ => seq(
            $._primary_key,
            field('column_list', $.column_list),
        ),

        _key_constraint: $ => seq(
            optional(
                choice(
                    $.keyword_unique,
                    $.keyword_foreign,
                ),
            ),
            choice($.keyword_key, $.keyword_index),
            optional(field('name', $.identifier)),
            $.ordered_columns,
            optional(
                seq(
                    $.keyword_references,
                    $.object_reference,
                    $.ordered_columns,
                    optional(
                        seq(
                            $.keyword_on,
                            $.keyword_delete,
                            $.keyword_cascade,
                        ),
                    ),
                ),
            ),
        ),

        ordered_columns: $ => paren_list(alias($.ordered_column, $.column), true),

        ordered_column: $ => seq(
            field('name', $.column),
            optional($.direction),
        ),

        all_fields: $ => seq(
            optional(
                seq(
                    $.object_reference,
                    '.',
                ),
            ),
            '*',
        ),

        parameter: $ => choice(
            "?",
            seq("$", RegExp("[0-9]+")),
        ),

        case: $ => seq(
            $.keyword_case,
            choice(
                // simplified CASE x WHEN
                seq(
                    $.expression,
                    $.keyword_when,
                    $.expression,
                    $.keyword_then,
                    $.expression,
                    repeat(
                        seq(
                            $.keyword_when,
                            $.expression,
                            $.keyword_then,
                            $.expression,
                        )
                    ),
                ),
                // standard CASE WHEN x, where x must be a predicate
                seq(
                    $.keyword_when,
                    $.expression,
                    $.keyword_then,
                    $.expression,
                    repeat(
                        seq(
                            $.keyword_when,
                            $.expression,
                            $.keyword_then,
                            $.expression,
                        )
                    ),
                ),
            ),
            optional(
                seq(
                    $.keyword_else,
                    $.expression,
                )
            ),
            $.keyword_end,
        ),

        field: $ => field('identifier_name', $.identifier),

        qualified_field: $ => seq(
            optional(
                seq(
                    optional_parenthesis($.object_reference),
                    '.',
                ),
            ),
            field('identifier_name', $.identifier),
        ),

        implicit_cast: $ => seq(
            $.expression,
            '::',
            $.data_type,
        ),

        interval_definitions: $ => repeat1(
            $._interval_definition
        ),

        _interval_definition: $ => seq(
            $.natural_number,
            choice(
                "millennium",
                "century",
                "decade",
                "year",
                "month",
                "week",
                "day",
                "hour",
                "minute",
                "second",
                "millisecond",
                "microsecond",
                "y",
                "m",
                "d",
                "H",
                "M",
                "S",
                "years",
                "months",
                "weeks",
                "days",
                "hours",
                "minutes",
                "seconds",
            ),
            optional(
                "ago",
            ),
        ),

        // Postgres syntax for intervals
        interval: $ => seq(
            $.keyword_interval,
            seq(
                "'",
                $.interval_definitions,
                "'",
            ),
        ),

        cast: $ => seq(
            field('name', $.keyword_cast),
            wrapped_in_parenthesis(
                seq(
                    field('parameter', $.expression),
                    $.keyword_as,
                    $.data_type,
                ),
            ),
        ),

        filter_expression: $ => seq(
            $.keyword_filter,
            wrapped_in_parenthesis($.where),
        ),

        invocation: $ => prec(1,
            seq(
                $.object_reference,
                choice(
                    // default invocation
                    paren_list(
                        seq(
                            optional($.keyword_distinct),
                            field(
                                'parameter',
                                $.term,
                            ),
                            optional($.order_by)
                        )
                    ),
                    // _aggregate_function, e.g. group_concat
                    wrapped_in_parenthesis(
                        seq(
                            optional($.keyword_distinct),
                            field('parameter', $.term),
                            optional($.order_by),
                            optional(seq(
                                choice($.keyword_separator, ','),
                                alias($.literal_string, $.literal)
                            )),
                            optional($.limit),
                        ),
                    ),
                ),
                optional(
                    $.filter_expression
                )
            ),
        ),

        exists: $ => seq(
            $.keyword_exists,
            $.subquery,
        ),

        partition_by: $ => seq(
            $.keyword_partition,
            $.keyword_by,
            comma_list($.expression, true),
        ),

        frame_definition: $ => seq(
            choice(
                seq(
                    $.keyword_unbounded,
                    $.keyword_preceding,
                ),
                seq(
                    field("start",
                        choice(
                            $.identifier,
                            $.binary_expression,
                            alias($.literal_string, $.literal),
                            alias($.integer, $.literal)
                        )
                    ),
                    $.keyword_preceding,
                ),
                $._current_row,
                seq(
                    field("end",
                        choice(
                            $.identifier,
                            $.binary_expression,
                            alias($.literal_string, $.literal),
                            alias($.integer, $.literal)
                        )
                    ),
                    $.keyword_following,
                ),
                seq(
                    $.keyword_unbounded,
                    $.keyword_following,
                ),
            ),
        ),

        window_frame: $ => seq(
            choice(
                $.keyword_range,
                $.keyword_rows,
                $.keyword_groups,
            ),

            choice(
                seq(
                    $.keyword_between,
                    $.frame_definition,
                    optional(
                        seq(
                            $.keyword_and,
                            $.frame_definition,
                        )
                    )
                ),
                seq(
                    $.frame_definition,
                )
            ),
            optional(
                choice(
                    $._exclude_current_row,
                    $._exclude_group,
                    $._exclude_ties,
                    $._exclude_no_others,
                ),
            ),
        ),

        window_clause: $ => seq(
            $.keyword_window,
            $.identifier,
            $.keyword_as,
            $.window_specification,
        ),

        window_specification: $ => wrapped_in_parenthesis(
            seq(
                optional($.partition_by),
                optional($.order_by),
                optional($.window_frame),
            ),
        ),

        window_function: $ => seq(
            $.invocation,
            $.keyword_over,
            choice(
                $.identifier,
                $.window_specification,
            ),
        ),

        alias_name: $ => seq(
            optional($.keyword_as),
            field('alias', $.identifier),
        ),

        from: $ => seq(
            $.keyword_from,
            optional(
                $.keyword_only,
            ),
            field("relation", $.relation),
            optional(field("where", $.where)),
        ),


        relation: $ => field("object_reference", $.object_reference),


        values: $ => seq(
            $.keyword_values,
            $.list,
            optional(
                repeat(
                    seq(
                        ',',
                        $.list,
                    ),
                ),
            ),
        ),

        index_hint: $ => seq(
            choice(
                $.keyword_force,
                $.keyword_use,
                $.keyword_ignore,
            ),
            $.keyword_index,
            optional(
                seq(
                    $.keyword_for,
                    $.keyword_join,
                ),
            ),
            wrapped_in_parenthesis(
                field('index_name', $.identifier),
            ),
        ),

        join: $ => seq(
            optional(
                choice(
                    $.keyword_left,
                    seq($.keyword_full, $.keyword_outer),
                    seq($.keyword_left, $.keyword_outer),
                    $.keyword_right,
                    seq($.keyword_right, $.keyword_outer),
                    $.keyword_inner,
                    $.keyword_full,
                ),
            ),
            $.keyword_join,
            $.relation,
            optional($.index_hint),
            optional($.join),
            choice(
                seq(
                    $.keyword_on,
                    field("predicate", $.expression),
                ),
                seq(
                    $.keyword_using,
                    alias($.column_list, $.list),
                )
            )
        ),

        cross_join: $ => seq(
            $.keyword_cross,
            $.keyword_join,
            $.relation,
        ),

        lateral_join: $ => seq(
            optional(
                choice(
                    // lateral joins cannot be right!
                    $.keyword_left,
                    seq($.keyword_left, $.keyword_outer),
                    $.keyword_inner,
                ),
            ),
            $.keyword_join,
            $.keyword_lateral,
            choice(
                $.invocation,
                $.subquery,
            ),
            optional(
                choice(
                    seq(
                        $.keyword_as,
                        field('alias', $.identifier),
                    ),
                    field('alias', $.identifier),
                ),
            ),
            $.keyword_on,
            choice(
                $.expression,
                $.keyword_true,
                $.keyword_false,
            ),
        ),

        lateral_cross_join: $ => seq(
            $.keyword_cross,
            $.keyword_join,
            $.keyword_lateral,
            choice(
                $.invocation,
                $.subquery,
            ),
            optional(
                choice(
                    seq(
                        $.keyword_as,
                        field('alias', $.identifier),
                    ),
                    field('alias', $.identifier),
                ),
            ),
        ),

        where: $ => seq(
            $.keyword_where,
            field("predicate", $.expression),
        ),

        group_by: $ => seq(
            $.keyword_group,
            $.keyword_by,
            comma_list($.expression, true),
            optional($._having),
        ),

        _having: $ => seq(
            $.keyword_having,
            $.expression,
        ),

        order_by: $ => prec.right(seq(
            $.keyword_order,
            $.keyword_by,
            comma_list($.order_target, true),
        )),

        order_target: $ => seq(
            $.expression,
            optional(
                seq(
                    choice(
                        $.direction,
                        seq(
                            $.keyword_using,
                            choice('<', '>', '<=', '>='),
                        ),
                    ),
                    optional(
                        seq(
                            $.keyword_nulls,
                            choice(
                                $.keyword_first,
                                $.keyword_last,
                            ),
                        ),
                    ),
                ),
            ),
        ),

        limit: $ => seq(
            $.keyword_limit,
            $.literal,
            optional($.offset),
        ),

        offset: $ => seq(
            $.keyword_offset,
            $.literal,
        ),

        returning: $ => seq(
            $.keyword_returning,
            $.select_expression,
        ),

        expression: $ => prec(1,
            choice(
                field("literal", $.literal),
                field("parameter_placeholder", $.parameter),
                field('qualified_field', $.qualified_field),
                field('binary_expression', $.binary_expression),
                field('between_expression', $.between_expression),
                wrapped_in_parenthesis(field('expression_in_parenthesis', $.expression)),
            )
        ),

        binary_expression: $ => choice(
            ...[
                ['+', 'binary_plus'],
                ['-', 'binary_plus'],
                ['*', 'binary_times'],
                ['/', 'binary_times'],
                ['%', 'binary_times'],
                ['^', 'binary_exp'],
                ['||', 'binary_concat'],
                ['=', 'binary_relation'],
                ['<', 'binary_relation'],
                ['<=', 'binary_relation'],
                ['!=', 'binary_relation'],
                ['>=', 'binary_relation'],
                ['>', 'binary_relation'],
                ['<>', 'binary_relation'],
                ['->', 'binary_relation'],
                ['->>', 'binary_relation'],
                ['#>', 'binary_relation'],
                ['#>>', 'binary_relation'],
                [$.keyword_is, 'binary_is'],
                [$.is_not, 'binary_is'],
                [$.keyword_like, 'pattern_matching'],
                [$.not_like, 'pattern_matching'],
                [$.similar_to, 'pattern_matching'],
                [$.not_similar_to, 'pattern_matching'],
                // binary_is precedence disambiguates `(is not distinct from)` from an
                // `is (not distinct from)` with a unary `not`
                [$.distinct_from, 'binary_is'],
                [$.not_distinct_from, 'binary_is'],
            ].map(([operator, precedence]) =>
                prec.left(precedence, seq(
                    field('left', $.expression),
                    field('operator', operator),
                    field('right', $.expression)
                ))
            ),
            ...[
                [$.keyword_and, 'clause_connective'],
                [$.keyword_or, 'clause_disjunctive'],
            ].map(([operator, precedence]) =>
                prec.left(precedence, seq(
                    field('left', $.expression),
                    field('operator', operator),
                    field('right', $.expression)
                ))
            ),
            ...[
                [$.keyword_in, 'binary_in'],
                [$.not_in, 'binary_in'],
            ].map(([operator, precedence]) =>
                prec.left(precedence, seq(
                    field('left', $.expression),
                    field('operator', operator),
                    field('right', choice($.list, $.subquery))
                ))
            ),
        ),

        unary_expression: $ => choice(
            ...[
                [$.keyword_not, 'unary_not'],
                [$.bang, 'unary_not'],
                [$.keyword_any, 'unary_not'],
                [$.keyword_some, 'unary_not'],
                [$.keyword_all, 'unary_not'],
            ].map(([operator, precedence]) =>
                prec.left(precedence, seq(
                    field('operator', operator),
                    field('operand', $.expression)
                ))
            ),
        ),

        between_expression: $ => choice(
            ...[
                [$.keyword_between, 'between'],
                [seq($.keyword_not, $.keyword_between), 'between'],
            ].map(([operator, precedence]) =>
                prec.left(precedence, seq(
                    field('left', $.expression),
                    field('operator', operator),
                    field('low', $.expression),
                    $.keyword_and,
                    field('high', $.expression)
                ))
            ),
        ),

        not_in: $ => seq(
            $.keyword_not,
            $.keyword_in,
        ),

        subquery: $ => wrapped_in_parenthesis(
            $.dml_read_stmt
        ),

        list: $ => paren_list(field('expression', $.expression)),

        literal: $ => prec(2,
            choice(
                field('integer', $.integer),
                field('decimal', $.decimal_number),
                field('string', $.literal_string),
                field('keyword_true', $.keyword_true),
                field('keyword_false', $.keyword_false),
                field('keyword_null', $.keyword_null),
            ),
        ),

        _double_quote_string: _ => seq('"', /[^"]*/, '"'),
        _single_quote_string: _ => seq("'", /([^']|'')*/, "'"),
        literal_string: $ => prec(1,
            choice(
                $._single_quote_string,
                $._double_quote_string,
            ),
        ),
        natural_number: _ => /\d+/,
        integer: $ => seq(optional("-"), $.natural_number),
        decimal_number: $ => choice(
            seq(optional("-"), ".", $.natural_number),
            seq($.integer, ".", $.natural_number),
            seq($.integer, "."),
        ),

        bang: _ => '!',

        identifier: $ => choice(
            $._identifier,
            $._double_quote_string,
            seq('`', $._identifier, '`'),
        ),
        _identifier: _ => /([a-zA-Z_][0-9a-zA-Z_]*)/,
    }

});

function unsigned_type($, type) {
    return choice(
        seq($.keyword_unsigned, type),
        seq(
            type,
            optional($.keyword_unsigned),
            optional($.keyword_zerofill),
        ),
    )
}

function optional_parenthesis(node) {
    return prec.right(
        choice(
            node,
            wrapped_in_parenthesis(node),
        ),
    )
}

function wrapped_in_parenthesis(node) {
    if (node) {
        return seq("(", node, ")");
    }
    return seq("(", ")");
}

function parametric_type($, type, params = ['size']) {
    return prec.right(1,
        choice(
            type,
            seq(
                type,
                wrapped_in_parenthesis(
                    seq(
                        // first parameter is guaranteed, shift it out of the array
                        field(params.shift(), alias($.natural_number, $.literal)),
                        // then, fill in the ", next" until done
                        ...params.map(p => seq(',', field(p, alias($.natural_number, $.literal)))),
                    ),
                ),
            ),
        ),
    )
}

function comma_list(field, requireFirst) {
    sequence = seq(field, repeat(seq(',', field)));

    if (requireFirst) {
        return sequence;
    }

    return optional(sequence);
}

function paren_list(field, requireFirst) {
    return wrapped_in_parenthesis(
        comma_list(field, requireFirst),
    )
}

function make_keyword(word) {
    str = "";
    for (var i = 0; i < word.length; i++) {
        str = str + "[" + word.charAt(i).toLowerCase() + word.charAt(i).toUpperCase() + "]";
    }
    return new RegExp(str);
}
