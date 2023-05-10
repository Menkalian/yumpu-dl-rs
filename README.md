# Tools for downloading pdfs from `yumpu.com`

> Note: This may only work with documents that are (freely) available without login.
> There is no support for overwriting the authentication.
> Also not all freely available files work. Try your luck.

# How to run
To run this program you need rust/cargo installed.
The downloader can be run with this command (execute in the root directory of the repository):

````shell
   $ cargo run -- <yumpu-url> <output-pdf-file>
````

An example to call this program is `cargo run -- https://www.yumpu.com/en/document/read/66625223/lebaron-manuals-92en lebaron-manual-92en.pdf`.
