# Background 
Instead of moving data everywhere, SaaS platforms needs to think about providing a marketplace that allows partners or even customers to bring software closer to the platform.

# Cold starts are lame

FaaS platform like lambda suffers from cold starts, that's unacceptable. There are techniques to warm it up, but that's just not good enough.  

A way to address this is running the code on v8 isolates. 

Cloudfare workers uses this technique.


# How 

1. library that allows you to run v8 isolates - https://github.com/rogchap/v8go
2. need to isolate so that it's not possible to acces file system - not sure how yet
3. attach to an events system