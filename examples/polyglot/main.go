package main

import (
	"fmt"
	"runtime"
)

func main() {
	fmt.Printf("PINSET_GO_OK %s\n", runtime.Version())
}
