# scs-user-api

User-facing API for SCS

## API Schema

All endpoints (except `/health`) require an auth token (Bearer), and they all return JSON.

<table>
  <tbody>
    <tr>
      <th>Endpoint</th>
      <th>Method</th>
      <th>URL path params</th>
      <th>URL query params</th>
      <th>Description</th>
    </tr>
    <tr>
      <td>`/health`</td>
      <td>`GET`</td>
      <td>None</td>
      <td>None</td>
      <td>Returns 200 OK, used to check if the API is running</td>
    </tr>
    <tr>
      <td>`/v1/logs/channels`</td>
      <td>`GET`</td>
      <td>None</td>
      <td>None</td>
      <td>Returns a list of logged channels</td>
    </tr>
    <tr>
      <td>`/v1/logs/{channel}`</td>
      <td>`GET`</td>
      <td>
        <ul>
          <li>`channel` - channel name (from the `/logs/channels` endpoint)</li>
        </ul>
      </td>
      <td>
        <ul>
          <li>`chatter` - filters for messages sent by this user</li>
          <li>`pattern` - filters for messages with a content that matches this [`LIKE`](https://www.postgresql.org/docs/14/functions-matching.html#FUNCTIONS-LIKE) pattern</li>
          <li>`cursor` - page token returned by the previous </li>
          <li>`page_size` - between 128 and 1024</li>
        </ul>
      </td>
      <td>Returns a paginated list of messages, and a cursor to retrieve the next page.</td>
    </tr>
  </tbody>
</table>
