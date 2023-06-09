# memex

Super simple "memory" for LLM projects, semantic search, etc.

<p style="text-align: center">
    <img src="docs/memex-in-action.gif">
</p>

## Running the service

Note that if you're running on Apple silicon (M1/M2/etc.), it's best to run natively (and faster)
since Linux ARM builds are very finicky.

``` bash
# Build and run the docker image
> docker compose up
# OR run natively in you have the rust toolchain installed.
> cargo run --release -p memex
```

## Add a document

``` bash
> curl http://localhost:8080/docs \
    -H "Content-Type: application/json" \
    --request POST \
    --data @example_docs/state_of_the_union_2023.json
```

## Run a query

``` bash
> curl http://localhost:8080/docs/search \
    -H "Content-Type: application/json" \
    --request GET \
    -d "{\"query\": \"what does Biden say about taxes?\", \"limit\": 3}"
```