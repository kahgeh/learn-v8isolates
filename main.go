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

type software struct {
	fileName        string
	originalContent string
}

func readUserJsFileName() string {
	path, _ := os.Getwd()
	reader := bufio.NewReader(os.Stdin)
	fmt.Println("Provide javascript file name to run:")
	text, _ := reader.ReadString('\n')
	return fmt.Sprintf("%s/%s", path, strings.Replace(text, "\n", "", -1))
}

func readEvent() string {
	reader := bufio.NewReader(os.Stdin)
	fmt.Println("Provide the event input to the javascript")
	text, _ := reader.ReadString('\n')
	return strings.Replace(text, "\n", "", -1)
}

func (app *software) getScript(event string) string {
	content := app.originalContent
	suffix := fmt.Sprintf(`main(JSON.parse('%s'),JSON.parse('{"user":"me"}'));`, event)
	script := fmt.Sprintf("%s\n%s", content, suffix)
	return script
}

func (app *software) run(event string) (result *v8go.Value, err error) {
	fileName := app.fileName
	script := app.getScript(event)
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

func newApp(content string, fileName string) *software {
	return &software{
		fileName:        fileName,
		originalContent: content,
	}
}

func main() {

	for {
		jsFileName := readUserJsFileName()
		event := readEvent()
		start := time.Now()

		content, err := ioutil.ReadFile(jsFileName)
		if err != nil {
			fmt.Printf("error reading javascript file %q, %s\n", jsFileName, err.Error())
			continue
		}
		app := newApp(string(content), jsFileName)
		loadingElapsed := time.Since(start)
		result, err := app.run(event)
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
