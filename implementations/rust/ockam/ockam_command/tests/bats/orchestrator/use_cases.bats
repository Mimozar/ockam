#!/bin/bash

# https://docs.ockam.io/use-cases

# ===== SETUP

setup() {
  load ../load/base.bash
  load_bats_ext
  setup_home_dir
  load ../load/orchestrator.bash
  skip_if_orchestrator_tests_not_enabled
  load ../load/docs.bash
  skip_if_docs_tests_not_enabled
  copy_enrolled_home_dir
}

teardown() {
  kill_kafka_contents || true
  kill_flask_server || true
  kill_telegraf_instance || true
  teardown_home_dir
}

# ===== TESTS

# https://docs.ockam.io/
# Please update the docs repository if this bats test is updated
@test "use-case - end-to-end encryption, orchestrator" {
  inlet_port="$(random_port)"
  relay_name="$(random_str)"

  # Service
  run_success "$OCKAM" tcp-outlet create --to $PYTHON_SERVER_PORT
  run_success "$OCKAM" relay create $relay_name

  # Client
  run_success $OCKAM tcp-inlet create --from "$inlet_port" --via "$relay_name"
  run_success curl --fail --head --retry-connrefused --retry-delay 5 --retry 10 --max-time 5 "127.0.0.1:$inlet_port"
}

# https://docs.ockam.io/use-cases/apply-fine-grained-permissions-with-attribute-based-access-control-abac
# Please update the docs repository if this bats test is updated
@test "use-case - abac" {
  port_1=$(random_port)
  port_2=$(random_port)
  relay_name=$(random_str)

  # Administrator
  ADMIN_HOME="$OCKAM_HOME"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=control --relay $relay_name > $OCKAM_HOME/control.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=edge > $OCKAM_HOME/edge.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=x > $OCKAM_HOME/x.ticket"

  # Control plane
  setup_home_dir
  run_success "$OCKAM" project import --project-file $PROJECT_PATH

  run_success $OCKAM identity create control_identity
  run_success $OCKAM project enroll "$ADMIN_HOME/control.ticket" --identity control_identity
  run_success $OCKAM node create control_plane1 --identity control_identity
  run_success $OCKAM tcp-outlet create --at /node/control_plane1 \
    --to 127.0.0.1:$PYTHON_SERVER_PORT --allow '(= subject.component "edge")'
  run_success $OCKAM relay create "$relay_name" --at /project/default --to /node/control_plane1

  # Edge plane
  setup_home_dir
  run_success "$OCKAM" project import --project-file $PROJECT_PATH

  $OCKAM identity create edge_identity
  $OCKAM project enroll "$ADMIN_HOME/edge.ticket" --identity edge_identity
  $OCKAM node create edge_plane1 --identity edge_identity
  $OCKAM tcp-inlet create --at /node/edge_plane1 --from "127.0.0.1:$port_1" \
    --via "$relay_name" --allow '(= subject.component "control")'
  run_success curl --fail --head --retry-connrefused --retry-delay 5 --retry 10 --max-time 5 "127.0.0.1:$port_1"

  ## The following is denied
  $OCKAM identity create x_identity
  $OCKAM project enroll "$ADMIN_HOME/x.ticket" --identity x_identity
  $OCKAM node create x --identity x_identity
  $OCKAM tcp-inlet create --at /node/x --from "127.0.0.1:$port_2" \
    --via "$relay_name" --allow '(= subject.component "control")'
  run curl --fail --head --max-time 5 "127.0.0.1:$port_2"
  assert_failure 28 # timeout error
}

# https://docs.ockam.io/guides/examples/basic-web-app
# Please update the docs repository if this bats test is updated
@test "use-case - basic web-app, single machine" {
  MACHINE_A="$OCKAM_HOME"
  export OCKAM_PG_PORT=$(random_port)
  relay_name=$(random_str)
  run_success $OCKAM tcp-outlet create --to "$PG_HOST:$PG_PORT"
  run_success $OCKAM relay create "$relay_name"

  run_success $OCKAM tcp-inlet create --from $OCKAM_PG_PORT --via "$relay_name"

  # Kickstart webserver
  export FLASK_PORT="$(random_port)"
  export APP_PG_PORT="$OCKAM_PG_PORT"
  run_success start_python_server

  # Visit website
  run_success curl --fail --max-time 5 "http://127.0.0.1:$FLASK_PORT"
  assert_output --partial "I've been visited 1 times"

  # Visit website second time
  run_success curl --fail --max-time 5 "http://127.0.0.1:$FLASK_PORT"
  assert_output --partial "I've been visited 2 times"

  run_success kill_flask_server
}

@test "use-case - basic-web-app, multiple machines" {
  MACHINE_A="$OCKAM_HOME"
  relay_name=$(random_str)

  # On machine A
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=db --relay ${relay_name} > ${MACHINE_A}/db.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=web > ${MACHINE_A}/webapp.ticket"

  # Machine B
  setup_home_dir
  run_success $OCKAM identity create db
  run_success $OCKAM project enroll "${MACHINE_A}/db.ticket" --identity db
  run_success $OCKAM node create db --identity db
  run_success $OCKAM tcp-outlet create --to "$PG_HOST:$PG_PORT" --allow '(= subject.component "web")'
  run_success $OCKAM relay create "$relay_name"

  # Machine C
  setup_home_dir
  export OCKAM_PG_PORT_MACHINE_C=$(random_port)
  run_success $OCKAM identity create web
  run_success $OCKAM project enroll ${MACHINE_A}/webapp.ticket --identity web
  run_success $OCKAM node create web --identity web
  run_success $OCKAM tcp-inlet create --from "$OCKAM_PG_PORT_MACHINE_C" --via $relay_name --allow '(= subject.component "db")'

  export FLASK_PORT="$(random_port)"
  export APP_PG_PORT="$OCKAM_PG_PORT_MACHINE_C"
  run_success start_python_server

  # Visit website
  run_success curl --fail --max-time 5 "http://127.0.0.1:$FLASK_PORT"
  assert_output --partial "I've been visited 3 times"
  # Visit website second time
  run_success curl --fail --max-time 5 "http://127.0.0.1:$FLASK_PORT"
  assert_output --partial "I've been visited 4 times"
}

# https://docs.ockam.io/guides/examples/okta
# Please update the docs repository if this bats test is updated
@test "use-case - okta" {
  skip "not yet finalized" # We require an okta login we performing ockam enroll --okta, enrolling automatically isn't supported right now

  ADMIN_HOME="$OCKAM_HOME"
  run_success "$OCKAM" project addon configure okta \
    --tenant "$OKTA_TENANT" --client-id "$OKTA_CLIENT_ID" \
    --attribute email --attribute city --attribute department

  run_success bash -c "$OCKAM project information --output json > project.json"

  # Generate enrollment tickets
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute application='Smart Factory' --attribute city='San Francisco' --relay m1 > m1.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute application='Smart Factory' --attribute city='New York' --relay m2 > m2.ticket"

  # Machine 1
  setup_home_dir
  run_success "$OCKAM" identity create m1
  run_success "$OCKAM" project enroll m1.ticket --identity m1
  run_success "$OCKAM" node create m1 --identity m1
  run_success "$OCKAM" tcp-outlet create --at /node/m1 --to 127.0.0.1:$PYTHON_SERVER_PORT \
    --allow '(or (= subject.application "Smart Factory") (and (= subject.department "Field Engineering") (= subject.city "San Francisco")))'
  run_success "$OCKAM" relay create m1 --at /project/default --to /node/m1

  # Machine 2
  setup_home_dir
  run_success "$OCKAM" identity create m2
  run_success "$OCKAM" project enroll m2.ticket --identity m2
  run_success "$OCKAM" node create m2 --identity m2
  run_success "$OCKAM" tcp-outlet create --at /node/m2 --to 127.0.0.1:6000 \
    --allow '(or (= subject.application "Smart Factory") (and (= subject.department "Field Engineering") (= subject.city "New York")))'
  run_success "$OCKAM" relay create m2 --at /project/default --to /node/m2

  # Alice
  setup_home_dir
  run_success "$OCKAM" project import --project-file project.json
  run_success "$OCKAM" project enroll --okta
  run_success "$OCKAM" node create alice
  run_success "$OCKAM" policy create --at alice --resource tcp-inlet

  # Alice request to access Machine 1 in San Francisco is allowed
  run_success "$OCKAM" tcp-inlet create --at /node/alice --from 127.0.0.1:8000 --via m1 --allow '(= subject.application "Smart Factory")'
  run_success curl --fail --head --retry-connrefused --retry-delay 5 --retry 10 --max-time 5 127.0.0.1:8000

  # Alice request to access Machine 2 in New York is denied
  run_success "$OCKAM" tcp-inlet create --at /node/alice --from 127.0.0.1:9000 --via m2 --allow '(= subject.application "Smart Factory")'
  run_failure curl --fail --head --max-time 3 127.0.0.1:9000
}

# https://docs.ockam.io/guides/examples/end-to-end-encrypted-kafka
# Please update the docs repository if this bats test is updated
@test "use-case - end-to-end-encrypted-kafka" {
  # Admin
  export ADMIN_HOME="$OCKAM_HOME"
  run_success "$OCKAM" project addon configure confluent --bootstrap-server "$CONFLUENT_CLOUD_BOOTSTRAP_SERVER_ADDRESS"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute role=member --relay '*' > ${ADMIN_HOME}/consumer.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute role=member --relay '*' > ${ADMIN_HOME}/producer1.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute role=member --relay '*' > ${ADMIN_HOME}/producer2.ticket"

  export CONSUMER_OUTPUT="$ADMIN_HOME/consumer.log"
  export KAFKA_CONFIG="$ADMIN_HOME/kafka.config"

  cat >"$KAFKA_CONFIG" <<EOF
request.timeout.ms=30000
security.protocol=SASL_PLAINTEXT
sasl.mechanism=PLAIN
sasl.jaas.config=org.apache.kafka.common.security.plain.PlainLoginModule required \
        username="$CONFLUENT_CLOUD_KAFKA_CLUSTER_API_KEY" \
        password="$CONFLUENT_CLOUD_KAFKA_CLUSTER_API_SECRET";
EOF

  export DEMO_TOPIC="$(random_str)"

  # Consumer
  setup_home_dir
  run_success "$OCKAM" identity create consumer
  run_success "$OCKAM" project enroll "${ADMIN_HOME}/consumer.ticket" --identity consumer
  run_success "$OCKAM" node create consumer --identity consumer
  run_success "$OCKAM" kafka-consumer create --at consumer

  run kafka-topics.sh --bootstrap-server localhost:4000 --command-config "$KAFKA_CONFIG" --create --topic "$DEMO_TOPIC" --partitions 3
  kafka-console-consumer.sh --topic "$DEMO_TOPIC" \
    --bootstrap-server localhost:4000 --consumer.config "$KAFKA_CONFIG" >"$CONSUMER_OUTPUT" 2>&1 &

  consumer_pid="$!"
  echo "$consumer_pid" >"$ADMIN_HOME/kafka.pid"

  # Producer 1
  run_success "$OCKAM" identity create producer1
  run_success "$OCKAM" project enroll "${ADMIN_HOME}/producer1.ticket" --identity producer1
  run_success "$OCKAM" node create producer1 --identity producer1
  run_success "$OCKAM" kafka-producer create --at producer1 --bootstrap-server 127.0.0.1:6000

  run bash -c "echo 'Hello from producer 1' | kafka-console-producer.sh --topic $DEMO_TOPIC \
    --bootstrap-server localhost:6000 \
    --producer.config $KAFKA_CONFIG"

  run_success cat $CONSUMER_OUTPUT
  assert_output "Hello from producer 1"

  # Producer 2
  setup_home_dir
  run_success "$OCKAM" identity create producer2
  run_success "$OCKAM" project enroll "${ADMIN_HOME}/producer2.ticket" --identity producer2
  run_success "$OCKAM" node create producer2 --identity producer2

  run_success "$OCKAM" kafka-producer create --at producer2 \
    --bootstrap-server 127.0.0.1:7000

  run_success bash -c "echo 'Hello from producer 2' | kafka-console-producer.sh --topic $DEMO_TOPIC \
    --bootstrap-server localhost:7000 \
    --producer.config $KAFKA_CONFIG"

  run_success cat $CONSUMER_OUTPUT
  assert_output --partial "Hello from producer 2"
}

# https://docs.ockam.io/guides/examples/telegraf-+-influxdb
# Please update the docs repository if this bats test is updated
@test "use-case - Telegraf + InfluxDB" {
  export ADMIN_HOME="$OCKAM_HOME"
  run_success start_telegraf_instance
  relay_name=$(random_str)

  # Ensure that telegraf works without using Ockam route
  run_success curl \
    --header "Authorization: Token $INFLUX_TOKEN" \
    --header "Accept: application/csv" \
    --header 'Content-type: application/vnd.flux' \
    --data "from(bucket:\"$INFLUX_BUCKET\") |> range(start:-1m)" \
    "http://localhost:$INFLUX_PORT/api/v2/query?org=$INFLUX_ORG"

  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=influxdb --relay $relay_name > ${ADMIN_HOME}/influxdb.ticket"
  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute component=telegraf > ${ADMIN_HOME}/telegraf.ticket"

  # InfluxDB instance
  setup_home_dir
  run_success "$OCKAM" identity create influxdb
  ockam project enroll "${ADMIN_HOME}/influxdb.ticket" --identity influxdb
  run_success "$OCKAM" node create influxdb --identity influxdb
  run_success "$OCKAM" tcp-outlet create --at /node/influxdb \
    --to "127.0.0.1:${INFLUX_PORT}" --allow '(= subject.component "telegraf")'
  run_success "$OCKAM" relay create $relay_name --at /project/default --to /node/influxdb

  # Telegraf instance
  setup_home_dir
  export INFLUX_PORT="$(random_port)"

  run_success "$OCKAM" identity create telegraf
  run_success "$OCKAM" project enroll "${ADMIN_HOME}/telegraf.ticket" --identity telegraf
  run_success "$OCKAM" node create telegraf --identity telegraf
  run_success "$OCKAM" tcp-inlet create --at /node/telegraf --from "127.0.0.1:${INFLUX_PORT}" \
    --via $relay_name --allow '(= subject.component "influxdb")'

  run_success kill_telegraf_instance
  run_success start_telegraf_instance

  # Ensure that telegraf works with using Ockam route
  run_success curl \
    --header "Authorization: Token $INFLUX_TOKEN" \
    --header "Accept: application/csv" \
    --header 'Content-type: application/vnd.flux' \
    --data "from(bucket:\"$INFLUX_BUCKET\") |> range(start:-1m)" \
    "http://localhost:$INFLUX_PORT/api/v2/query?org=$INFLUX_ORG"
}

# https://docs.ockam.io/guides/examples/okta
# Please update the docs repository if this bats test is updated
@test "use-case - InfluxDB Cloud token lease management" {
  skip "Influx DB needs a fix" # Not working currently

  export INFLUXDB_LEASE_PERMISSIONS="[{\"action\":  \"read\", \"resource\": {\"type\": \"authorizations\", \"orgID\": \"$INFLUXDB_ORG_ID\"}}]"
  export ADMIN_HOME="$OCKAM_HOME"

  run_success "$OCKAM" project addon configure influxdb \
    --endpoint-url "$INFLUXDB_ENDPOINT_URL" \
    --token "$INFLUXDB_ADMIN_TOKEN" \
    --org-id "$INFLUXDB_ORG_ID" \
    --permissions "$INFLUXDB_LEASE_PERMISSIONS" \
    --max-ttl 900

  run_success bash -c "$OCKAM project ticket --usage-count 10 --attribute service=iot-sensor > ${ADMIN_HOME}/sensor.ticket"

  # Client
  setup_home_dir
  run_success "$OCKAM" identity create iot-sensor
  run_success "$OCKAM" project enroll "${ADMIN_HOME}/sensor.ticket" --identity iot-sensor
  run_success "$OCKAM" lease create --identity iot-sensor
}
