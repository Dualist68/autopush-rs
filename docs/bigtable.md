# Using bigtable emulation for testing

The bigtable emulator is memory only. No structure persists after
restart.

## Getting the SDK

see [the Google Cloud CLI installation page](https://cloud.google.com/sdk/docs/install#deb)

for docker:
google/cloud-sdk:latest <!-- TODO: need to work out details for connection to this. -->

## Starting the emulator

`gcloud beta emulators bigtable start &`
Starts the emulator on localhost:8086 (use `--host-port` to change
this)

Next export the `BIGTABLE_EMULATOR_HOST` signature to the local environment.

The documentation says to run
`$(gcloud beta emulators bigtable env-init)`
however this will read a file in the gcloud directory. If you are runnning the emulator in a docker image and have not connected a volume, this command will fail. You can still
manually export the variable with the host and port to use
`export BIGTABLE_EMULATOR_HOST=localhost:8086`

## Initialization

`gcloud components install cbt`

This will install the `cbt` command to the directory that the gcloud SDK was installed into. Make sure that this directory is in your environment PATH

you can then use the following commands:

```bash
BIGTABLE_EMULATOR_HOST=localhost:8086 \
    cbt -project test -instance test createtable autopush && \
    cbt -project test -instance test createfamily autopush router && \
    cbt -project test -instance test createfamily autopush message && \
    cbt -project test -instance test createfamily autopush message_topic && \
    cbt -project test -instance test setgcpolicy autopush router maxversions=1 && \
    cbt -project test -instance test setgcpolicy autopush message maxversions=1 and maxage=1s && \
    cbt -project test -instance test setgcpolicy autopush message_topic maxversions=1 and maxage=1s
```

This will create a new project named `test`, a new instance named `test` and a new table named `autopush`, along with column family definitions for `messsage` and `router`.

## *TODO*

* Not quite sure how to configure the various docker bits to do a lot of this. I can run a docker-compose to bring up an emulator, but getting `cbt` to talk to it reliably fails.
* Running the emulator in a docker image blocks the cbt from connecting correctly?
