package main

import (
	"bufio"
	"fmt"
	"io/ioutil"
	"os"
	"strings"
	"time"

	"rogchap.com/v8go"
)

func readUserJsFileName() string {
	path, _ := os.Getwd()
	reader := bufio.NewReader(os.Stdin)
	fmt.Println("Provide javascript file name to run:")
	text, _ := reader.ReadString('\n')
	return fmt.Sprintf("%s/%s", path, strings.Replace(text, "\n", "", -1))
}

func runJs(script string, fileName string) (result *v8go.Value, err error) {

	ctx, _ := v8go.NewContext(nil) // creates a new V8 context with a new Isolate aka VM
	vals := make(chan *v8go.Value, 1)
	errs := make(chan error, 1)
	go func() {
		val, err := ctx.RunScript(script, fileName) // exec a long running script
		if err != nil {
			errs <- err
			return
		}
		vals <- val
	}()
	select {
	case result := <-vals:
		return result, nil
		// sucess
	case err := <-errs:
		return nil, err
	case <-time.After(200 * time.Millisecond):
		vm, _ := ctx.Isolate()
		vm.TerminateExecution()
		err := <-errs
		return nil, err
	}
}

func main() {

	for {
		jsFileName := readUserJsFileName()
		start := time.Now()
		content, err := ioutil.ReadFile(jsFileName)
		if err != nil {
			fmt.Printf("error reading javascript file %q, %s\n", jsFileName, err.Error())
			continue
		}
		loadingElapsed := time.Since(start)
		result, err := runJs(string(content), jsFileName)
		if err != nil {
			fmt.Printf("error running javascript file %q, %s\n", jsFileName, err.Error())
			continue
		}
		e2eElapsed := time.Since(start)
		fmt.Printf("%q's execution\n", jsFileName)
		fmt.Printf("result : %s\n", result.String())
		fmt.Printf("elapsed time: %v\n", e2eElapsed)
		fmt.Printf("loading time: %v\n", loadingElapsed)
	}

}
