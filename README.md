# routeros-steamcm-iplist

A simple tool to make the firewall/policy routing of Steam CM servers on Mikrotik routers easier.

## Features

- Fetches Steam's API to obtain a list of CM servers for current IP.
- Generates a Mikrotik RouterOS (ROS) configuration script based on the obtained CM server information.
- Supports two modes: Automatic ROS configuration update with ROS [REST API](https://help.mikrotik.com/docs/display/ROS/REST+API) and HTTP server for ROS script retrieval.
- Designed to run inside a container on Mikrotik routers.

## Why make this tool?

I found that there is no optimal solution on Mikrotik Routers to apply firewall/routing policies to Steam's CM (Connection Manager) servers that makes me happy enough. The two main problem I'm attempting to solve is:

1. CM servers change their IP addresses and domain names from time to time.
2. Steam utilizes a combination of domain name and IP-only connection methods to access its CM servers.

Previously, we have these methods to partially work around this:

- **Static CIDR Based Routing:** Use broad IP CIDR blocks to route traffic to CM servers and apply firewall rules to them. To reduce maintenance headache, the IP CIDR is usually very big to include all potential IPs that may be used by CM server, and may include IP ranges shared by non-CM server services, which compromising the efficiency of routing and applies wrong filter rules.

- **DNS Hijacking:** Hijacking the DNS resolution for wildcard domains of the CM servers, and point the domain to a reverse proxy. This becomes particularly problematic when Steam employs IP-only connections, bypassing DNS rules and resulting in inconsistent routing.

This tool addresses these challenges by fetching real-time server information and generating up-to-date configuration scripts.

## Usage

As this tool is designed to be run inside a container, all configuration is supplied by env variables.

To run it on RouterOS, make sure the [Container](https://help.mikrotik.com/docs/display/ROS/Container) function on your router is set up correctly.

Pre-built and tagged image in tarball for arm64 is available in the release page.

To build your own image for current arch:

```shell
docker build -t gnattu/update-steamcm-iplist:latest .
docker save gnattu/update-steamcm-iplist:latest > update-steammcm.tar
```

Or use `buildx` for other arch:

```shell
docker buildx build  --no-cache --platform linux/arm/v7 --output=type=docker -t gnattu/update-steamcm-iplist:latest .
docker save gnattu/update-steamcm-iplist:latest > update-steammcm.tar
```

Then you can import the generated tarball to your router. 

### Automatic ROS configuration update mode

To run in this mode, you need to correctly set the [REST API](https://help.mikrotik.com/docs/display/ROS/REST+API) of your router, then supply the following env variables:

```shell
export MIKROTIK_ADDRESS=<Mikrotik Router Address>
export MIKROTIK_USER=<Username>
export MIKROTIK_PASS=<Password>
```

and optionally:
```shell
export MIKROTIK_ADDRESS_LIST_NAME=<Address List Name>
```
to specify the target address list name you want.

After you imported the container, you can use the following script in the RouterOS scheduler to perform automatically updates: 

```
/container/start [find tag="gnattu/update-steamcm-iplist:latest"]
```

The update interval does not need to be very frequent, once per day is good enough for most use cases.

#### Security Notice:

For performance reason, this tool use `http` for REST API because it is considered safe for a host-container network. If you plan to deploy this outside of the router, be aware that **credentials are sent in plaintext**.

I personally don't think use `https` with this tool is a good idea due to Mikrotik limitations. The Mikrotik's REST API can only add or remove on item at a time, but we have to update ~50 ip addresses every time. This amount of tls handshakes easily stressed one A72 core on my RB5009 to 100%. If you believe your router is powerful enough to use `https`, please modify the code and compile your own.

### ROS configuration script server mode

This mode requires only one mandatory env variable:

```shell
export MIKROTIK_FETCH_PORT=<config_server_listen_port>
```

If the env `MIKROTIK_FETCH_PORT` is set to some non-empty value, this tool will run in server mode, listening this port on `0.0.0.0`. Any HTTP request to this port will get a RouterOS script which you can import to your router. 

You can also set a script in RouterOS scheduler like this to perform the import automatically:

```
/tool fetch url="http://<container-ip>:<MIKROTIK_FETCH_PORT>" dst-path=update-cm-list.rsc;
/import file-name=update-cm-list.rsc;
```

The update interval does not need to be very frequent, once per day is good enough for most use cases.


