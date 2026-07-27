{config, ...}:{
  imports = [
    ./hardware-configuration.nix
    ./age.nix
    ./disko.nix
    ./github.nix
  ];

  config = {
    nixpkgs.hostPlatform = "x86_64-linux";
    system.stateVersion = "25.05";
    boot.loader.systemd-boot.enable = true;
    boot.loader.efi.canTouchEfiVariables = true;
    nix.settings.experimental-features = ["nix-command" "flakes"];
    networking.hostName = "runner-server";
    services.avahi = {
      enable = true;
      publish = {
        enable = true;
        addresses = true;
      };
    };
    networking.wireless = {
      enable = true;
      interfaces = [ "wlp58s0" ];
      secretsFile = config.age.secrets.wifi.path;
      networks = builtins.listToAttrs
        (map (name: { inherit name; value.pskRaw = "ext:PSK_${name}"; })
            ["<YOUR SSID HERE>"]);
    };
    services.openssh.enable = true;
    virtualisation.vmVariant = {
      virtualisation = {
        memorySize = 20000; # use 2048MiB memory
        cores = 8;         # use 3 cpu cores
      };
    };
  };
}
