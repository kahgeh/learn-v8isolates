function fibonacci(num) {
    var a = 1, b = 0, temp;

    while (num >= 0) {
        temp = a;
        a = a + b;
        b = temp;
        num--;
    }

    return b;
}

function main(event, context) {
    return `request submitted by ${context.user} with parameter ${event.param1} = ${fibonacci(event.param1)}`;
}

