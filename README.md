nsupdate_cloudflare
===================

A simple yet dirty tool that can update your DNS records on CloudFlare with good old nsupdate commands.

I know the `src/parser.rs` and `src/nsupdate.pest` is dirty as fsck, but hey, it werks.


Usage
-----

```
nsupdate_cloudflare 0.1.0
A simple yet dirty tool to apply nsupdate commands to CloudFlare DNS

USAGE:
    nsupdate_cloudflare [FLAGS] --file <file> --zone <zone-id> --token <token>

FLAGS:
    -h, --help       Prints help information
    -v, --verbose    Verbose level
    -V, --version    Prints version information

OPTIONS:
    -f, --file <file>       Path to nsupdate file
    -t, --token <token>     Token retrieved from Cloudflare
    -z, --zone <zone-id>    Zone ID retrieved from Cloudflare
```
