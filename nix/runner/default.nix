{agenix, ...}:{
  imports = [
    agenix.nixosModules.default
    ./hardware-configuration.nix
    ./github.nix
  ];

  config = {
    nixpkgs.hostPlatform = "x86_64-linux";
    system.stateVersion = "25.05";
    boot.loader.systemd-boot.enable = true;
    boot.loader.efi.canTouchEfiVariables = true;
    nix.settings.experimental-features = ["nix-command" "flakes"];
    services.openssh.enable = true;
    virtualisation.vmVariant = {
      virtualisation = {
        memorySize = 20000; # use 2048MiB memory
        cores = 8;         # use 3 cpu cores
      };
    };
  };
}
