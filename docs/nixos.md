# Use with NixOS

For NixOS, there is a module available via the nix flake. You can include it in your configuration like this:

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-XX.YY";

  inputs.muscl.url = "git+https://git.pvv.ntnu.no/Projects/muscle.git";
  inputs.muscl.inputs.nixpkgs.follows = "nixpkgs";

  ...
}
```

The module allows for easy setup on a local machine by enabling `services.muscl.createLocalDatabaseUser`.
