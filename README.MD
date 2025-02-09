# bit-distribution-analyzer

[![Crates.io](https://img.shields.io/crates/v/bit-distribution-analyzer.svg)](https://crates.io/crates/bit-distribution-analyzer)
[![Docs.rs](https://docs.rs/bit-distribution-analyzer/badge.svg)](https://docs.rs/bit-distribution-analyzer)
[![CI](https://github.com/Sewer56/bit-distribution-analyzer/actions/workflows/rust.yml/badge.svg)](https://github.com/Sewer56/bit-distribution-analyzer/actions)

-----------------------

## Additional Setup Required!!

Additional setup might be required.  
Once you do the stuff under this text, delete this section.  

### Code Coverage
To setup code coverage for this project. Login into [Codecov][codecov] and add this repository from your list of tracked repos.


### Setup API Key (crates.io)

You'll need to set up an API key to publish to `crates.io` on tag.

- Go to `Settings -> Secrets -> Actions` in your repo and add environment secret named `CRATES_IO_TOKEN`.  
- Paste your API key generated in [crates.io (Account Settings -> API Tokens)][crates-io-key].  

### Fill in Crate Description

This readme file (under the dashed line) is used for the crate description on GitHub and
crates.io (if uploaded). Consider filling in this readme with examples for end users and any
other useful info.

-----------------------

## About

Analyzes the bit distribution and compression statistics of arrays of packed structures.

## Features

- Print entropy for every field and group
- Print bit distribution (1/0) for every bit
- Print distribution statistics grouped by field value

## Known Limitations

Will fix when suitable. I don't need those features for now.

- Structs have to be byte aligned.
- Frequency counting for fields above 64 bits in size is not supported.
    - Memory would blow up, and it's also really slow.

## Tips

The program uses a lot of memory.  
Expect at least twice the size of the input data; or more if the data contains a lot of nested groups.  

When running the tool on a very large data set you might want to set:

```bash
# on Linux
sudo sysctl vm.overcommit_memory=1
```

And create a large swapfile on your system.  
In my case, for 32GB of input, I used 32GiB of RAM + 128GiB of swap with complex schema.  
Otherwise the process may get killed during analysis.  

## Development

For information on how to work with this codebase, see [README-DEV.MD](README-DEV.MD).

## License

Licensed under [GPL v3 (with Reloaded FAQ)](./LICENSE).  

[Learn more about Reloaded's general choice of licensing for projects.][reloaded-license].  

[codecov]: https://about.codecov.io/
[crates-io-key]: https://crates.io/settings/tokens
[nuget-key]: https://www.nuget.org/account/apikeys
[reloaded-license]: https://reloaded-project.github.io/Reloaded.MkDocsMaterial.Themes.R2/Pages/license/