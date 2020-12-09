# Background

An attempt to understand what it takes to run v8 isolates.

## Why?

Instead of moving data everywhere, SaaS platforms needs to think about providing a marketplace that allows partners or even customers to bring software closer to the platform.

## Cold starts are lame

FaaS platform like lambda suffers from cold starts, that's unacceptable. There are techniques to warm it up, but that's just not good enough.

A way to address this is running the code on v8 isolates.

Cloudfare workers uses this technique.

# Examples

1. [go-v8](./go-v8/Readme.MD)

2. [rust-v8](./rust-v8/Readme.MD) - unlike `go` there is no garbage collector, I guess that adds to performance and stability because there will only be v8 garbage collector working and not the host program.
