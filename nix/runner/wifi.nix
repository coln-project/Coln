{ config, ... }:
{
  age.secrets.wifi =  {
    file = ./secrets/wifi.age;
    mode = "0440";
    group = "wpa_supplicant";
  };
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
    networks = builtins.listToAttrs (
      map
        (name: {
          inherit name;
          value.pskRaw = "ext:PSK_${name}";
        })
        [
          "<YOUR-SSID-HERE>"
        ]
    );
  };
}
