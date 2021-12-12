## Instructions

1. Run the binary
2. Open the browser and navigate to http://localhost:8080/playground (you can also use [Apollo Studio Explorer](https://studio.apollographql.com/sandbox/explorer) if you want)
3. Set the server URL to http://localhost:8080/graphql
4. Explore the schema by clicking on the "Schema" and "Docs" buttons on the right. Press Ctrl+P to get suggestions while editing the queries and inputs.

## Sample Queries

See the sample queries and inputs in the next two code blocks.

```graphql
# Info and metadata queries
query GetInfo($name: String!) {
  modelInfo(name: $name) {
    dateCreated
    dateModified
    isCompressed
    name
    size
  }
  channels {
    name
    totalSize
  }
  channel(name: $name) {
    name
    logFiles {
      name
      size
    }
    totalSize
  }
  models {
    size
    name
    dateModified
    dateCreated
  }
}

# This mutation (aka POST) loads a model into the memory
mutation LoadModel($name: String!) {
  loadModel(name: $name) {
    name
    size
    order
    metadata
  }
}

# This query uses a previously loaded model
query UseModel($name: String!, $input: ModelInput!) {
  modelMeta(name: $name) {
    metadata
    name
    order
    size
  }
  generateText(input: $input) {
    outputs {
      text
      numSamples
    }
    maxSamples
  }
}
```

Inputs:

```json
{
  "name": "ambadev.chain",
  "input": {
    "name": "ambadev.chain",
    "seedPhrase": "whats up",
    "maxSamples": 4,
    "nOutputs": 5
  }
}
```
