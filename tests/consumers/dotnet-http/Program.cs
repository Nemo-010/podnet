// T-358 step 3: the HTTP client of a consumer under the vnet shim.
// It does what the runner of podrunner does: HttpClient with the default
// SocketsHttpHandler (dual-mode IPv6 sockets) sends GET to
// http://127.0.0.1:PORT/. It prints the body. It exits 0 on HTTP 200,
// else 1.
if (args.Length != 1 || !int.TryParse(args[0], out var port))
{
    Console.Error.WriteLine("usage: dotnet-http PORT");
    return 64;
}
using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(20) };
try
{
    using var response = await client.GetAsync($"http://127.0.0.1:{port}/");
    var body = await response.Content.ReadAsStringAsync();
    Console.WriteLine($"status {(int)response.StatusCode}");
    Console.WriteLine(body);
    return response.StatusCode == System.Net.HttpStatusCode.OK ? 0 : 1;
}
catch (Exception e)
{
    Console.Error.WriteLine($"error: {e.GetType().Name}: {e.Message}");
    for (var inner = e.InnerException; inner != null; inner = inner.InnerException)
    {
        Console.Error.WriteLine($"inner: {inner.GetType().Name}: {inner.Message}");
    }
    return 1;
}
