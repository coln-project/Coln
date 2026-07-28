{agenix, ...}:{
  imports = [
    agenix.nixosModules.default
    ./github.nix
  ];

  config = {
    nixpkgs.hostPlatform = "x86_64-linux";
    system.stateVersion = "25.05";
    boot.loader.systemd-boot.enable = true;
    boot.loader.efi.canTouchEfiVariables = true;
    nix.settings.experimental-features = ["nix-command" "flakes"];
    services.openssh.enable = true;

    # VM Settings
    virtualisation.vmVariant = {
      virtualisation = {
        memorySize = 8000;
        cores = 8;
      };
    };
  };
}
