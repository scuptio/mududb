package main

import (
	"fmt"
	"github.com/mududb/mududb-go/adapter"
	"github.com/mududb/mududb-go/types"
	"os"
	"strconv"
	"strings"
)

var client *adapter.Client

func invoke(operation string, frame []byte) ([]byte, error) { return client.Call(operation, frame) }
func run() error {
	var err error
	client, err = adapter.Open("")
	if err != nil {
		return err
	}
	defer client.Close()
	request, err := types.EncodeOpenSessionRequest(types.UniOid{})
	if err != nil {
		return err
	}
	frame, err := client.Call("open", request)
	if err != nil {
		return err
	}
	opened, err := types.DecodeOpenSessionResult(frame)
	if err != nil {
		return err
	}
	if opened.Error != nil {
		return fmt.Errorf("%s", opened.Error.ErrMsg)
	}
	session := opened.Value.(types.UniOid)
	defer func() {
		request, _ := types.EncodeCloseSessionRequest(session)
		_, err := client.Call("close", request)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
		}
	}()
	if len(os.Args) < 2 {
		return fmt.Errorf("usage: init | scenario | create ID NAME EMAIL | balance ID | deposit ID AMOUNT | withdraw ID AMOUNT | transfer FROM TO AMOUNT")
	}
	if os.Args[1] == "init" {
		ddl, err := os.ReadFile("ddl.sql")
		if err != nil {
			return err
		}
		for _, sql := range strings.Split(string(ddl), ";") {
			if strings.TrimSpace(sql) != "" {
				if _, err = sysCommand(session, sql); err != nil {
					return err
				}
			}
		}
		return nil
	}
	if os.Args[1] == "scenario" {
		if _, err = createUser(session, 1001, "Alice", "alice@example.com"); err != nil {
			return err
		}
		if _, err = createUser(session, 1002, "Bob", "bob@example.com"); err != nil {
			return err
		}
		if _, err = deposit(session, 1001, 1000); err != nil {
			return err
		}
		if _, err = withdraw(session, 1001, 100); err != nil {
			return err
		}
		if _, err = transferFunds(session, 1001, 1002, 250); err != nil {
			return err
		}
		a, err := balance(session, 1001)
		if err != nil {
			return err
		}
		b, err := balance(session, 1002)
		if err != nil {
			return err
		}
		if a != 650 || b != 250 {
			return fmt.Errorf("balance mismatch: %d/%d", a, b)
		}
		if _, err = withdraw(session, 1001, 10000); err == nil || err.Error() != "insufficient funds" {
			return fmt.Errorf("unexpected withdrawal: %v", err)
		}
		fmt.Println(`{"alice":650,"bob":250}`)
		return nil
	}
	if len(os.Args) < 3 {
		return fmt.Errorf("missing user id")
	}
	id, err := strconv.ParseInt(os.Args[2], 10, 64)
	if err != nil {
		return err
	}
	var result int64
	switch os.Args[1] {
	case "balance":
		result, err = balance(session, id)
	case "create":
		if len(os.Args) != 5 {
			return fmt.Errorf("create ID NAME EMAIL")
		}
		result, err = createUser(session, id, os.Args[3], os.Args[4])
	case "deposit", "withdraw":
		if len(os.Args) != 4 {
			return fmt.Errorf("deposit/withdraw ID AMOUNT")
		}
		n, e := strconv.ParseInt(os.Args[3], 10, 64)
		if e != nil {
			return e
		}
		if os.Args[1] == "deposit" {
			result, err = deposit(session, id, n)
		} else {
			result, err = withdraw(session, id, n)
		}
	case "transfer":
		if len(os.Args) != 5 {
			return fmt.Errorf("transfer FROM TO AMOUNT")
		}
		to, e := strconv.ParseInt(os.Args[3], 10, 64)
		if e != nil {
			return e
		}
		n, e := strconv.ParseInt(os.Args[4], 10, 64)
		if e != nil {
			return e
		}
		result, err = transferFunds(session, id, to, n)
	default:
		return fmt.Errorf("unknown operation")
	}
	if err == nil {
		fmt.Println(result)
	}
	return err
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
