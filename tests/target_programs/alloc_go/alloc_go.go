package main

import (
	"fmt"
	"math/rand"
	"os"
	"time"
)

func goHeavyAlloc() {
	mb := 100 + rand.Intn(200) // 100-300 MB
	fmt.Printf("[alloc_go] allocating %d MB\n", mb)
	data := make([]byte, mb*1024*1024)
	// Touch every page to ensure it's resident
	for i := 0; i < len(data); i += 4096 {
		data[i] = byte(i)
	}
	_ = data
	time.Sleep(3 * time.Second)
}

func main() {
	fmt.Printf("[alloc_go] started pid=%d\n", os.Getpid())
	for {
		goHeavyAlloc()
		time.Sleep(1 * time.Second)
	}
}
