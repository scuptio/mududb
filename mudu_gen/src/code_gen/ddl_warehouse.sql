DROP TABLE IF EXISTS warehouse;
CREATE TABLE warehouse
(
    w_id       int            NOT NULL,
    w_ytd      decimal(12, 2) NOT NULL,
    w_tax      decimal(4, 4)  NOT NULL,
    w_name     varchar(10)    NOT NULL,
    w_street_1 varchar(20)    NOT NULL,
    w_street_2 varchar(20)    NOT NULL,
    w_city     varchar(20)    NOT NULL,
    w_state    char(2)        NOT NULL,
    w_zip      char(9)        NOT NULL,
    PRIMARY KEY (w_id)
);