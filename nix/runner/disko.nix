{disko, ...}:{
  imports = [disko.nixosModules.disko];
  config = {
    disko.devices = {
      disk = {
        main = {
          imageSize = "10G";
          device = "/dev/disk/by-id/ata-SK_hynix_SC311_SATA_256GB_MS82N176810802R0S";
          type = "disk";
          content = {
            type = "gpt";
            partitions = {
              ESP = {
                type = "EF00";
                size = "500M";
                content = {
                  type = "filesystem";
                  format = "vfat";
                  mountpoint = "/boot";
                  mountOptions = [ "umask=0077" ];
                };
              };
              root = {
                size = "100%";
                content = {
                  type = "filesystem";
                  format = "ext4";
                  mountpoint = "/";
                };
              };
            };
          };
        };
      };
    };
  };
}
