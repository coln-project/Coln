{agenix, ...}:{
  imports = [
    agenix.nixosModules.default
    ./github.nix
  ];

  config = {
    nixpkgs.hostPlatform = "x86_64-linux";
    system.stateVersion = "26.05";
    boot.loader.systemd-boot.enable = true;
    boot.loader.efi.canTouchEfiVariables = true;
    nix.settings = {
      experimental-features = ["nix-command" "flakes"];
      trusted-substituters = [ "https://coln.cachix.org" ];
      trusted-public-keys = [ "coln.cachix.org-1:xplHZrvUVve3NSquwwW5QRl6MYbDBHx3rw3Np69kjw4=" ];
    };
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
