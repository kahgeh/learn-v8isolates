async function getStuff(num) {

    const response = await fetch("https://google.com")
    return response.text()
}

async function main(event, context) {
    return `request submitted by ${context.user} with parameter ${event.param1} = ${await getStuff(event.param1)}`;
}

