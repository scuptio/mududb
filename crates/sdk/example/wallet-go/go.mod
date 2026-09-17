module wallet_go

go 1.24

require (
	github.com/ybbh/mududb_p/bindings/go v0.0.0-00010101000000-000000000000
	go.bytecodealliance.org/cm v0.3.0
)

// The MSSP wire runtime (types + codec) is the in-repo Go binding; cm is the
// wit-bindgen-go runtime, checked in under third_party/ so builds run offline.
replace (
	github.com/ybbh/mududb_p/bindings/go => ../../bindings/go
	go.bytecodealliance.org/cm => ./third_party/cm
)
