# frozen_string_literal: true

require 'fileutils'
require 'minitest/autorun'
require 'socket'
require 'stringio'
require 'tmpdir'

require_relative 'main'

# Exercises the public behavior of the local artifact server.
class LocalArtifactServerTest < Minitest::Test
  def setup
    @root = Dir.mktmpdir('upstream-server-test-')
    write_fixture
    @server = build_server
    @thread = Thread.new { @server.start }
  end

  def write_fixture
    File.binwrite(File.join(@root, 'tool.tar.gz'), 'artifact bytes')
  end

  def build_server
    LocalArtifactServer::HttpServer.new(
      host: '127.0.0.1', port: 0, artifact_root: @root,
      page_router: build_page_router, logger: StringIO.new
    )
  end

  def build_page_router
    router = LocalArtifactServer::PageRouter.new
    router.get('/download') do
      LocalArtifactServer::Response.new(status: 200, body: '<h1>Downloads</h1>', content_type: 'text/html')
    end
    router
  end

  def teardown
    @server.stop
    @thread.join
    FileUtils.remove_entry(@root)
  end

  def request(method, path, headers = {})
    socket = TCPSocket.new('127.0.0.1', @server.server.addr[1])
    fields = headers.map { |name, value| "#{name}: #{value}\r\n" }.join
    socket.write("#{method} #{path} HTTP/1.1\r\nHost: localhost\r\n#{fields}Connection: close\r\n\r\n")
    response = socket.read
    socket.close
    response
  end

  def test_serves_artifacts_with_mime_type
    response = request('GET', '/artifacts/tool.tar.gz')
    assert_includes response, 'HTTP/1.1 200 OK'
    assert_includes response, 'Content-Type: application/gzip'
    assert response.end_with?('artifact bytes')
  end

  def test_head_omits_body
    response = request('HEAD', '/artifacts/tool.tar.gz')
    assert_includes response, 'HTTP/1.1 200 OK'
    refute response.end_with?('artifact bytes')
  end

  def test_rejects_traversal_and_unknown_files
    assert_includes request('GET', '/artifacts/../secret'), '404 Not Found'
    assert_includes request('GET', '/artifacts/missing.tar.gz'), '404 Not Found'
  end

  def test_rejects_unsupported_methods
    assert_includes request('POST', '/artifacts/tool.tar.gz'), '405 Method Not Allowed'
  end

  def test_dispatches_page_routes
    response = request('GET', '/download')
    assert_includes response, 'HTTP/1.1 200 OK'
    assert response.end_with?('<h1>Downloads</h1>')
  end

  def fixture_routes(*routes)
    File.write(File.join(@root, '.routes.json'), JSON.generate(routes))
  end

  def test_fixture_routes_match_method_and_decoded_query_and_reload
    fixture_routes({ path: '/api/releases', method: 'GET', query: { search: 'paint & draw' },
                     body: '{"tag":"v1"}', content_type: 'application/json' })
    assert request('GET', '/api/releases?search=paint%20%26%20draw').end_with?('{"tag":"v1"}')
    assert_includes request('HEAD', '/api/releases?search=paint%20%26%20draw'), '404 Not Found'
    assert_includes request('GET', '/api/releases?search=other'), '404 Not Found'
    fixture_routes({ path: '/api/releases', body: '{"tag":"v2"}' })
    assert request('GET', '/api/releases').end_with?('{"tag":"v2"}')
  end

  def test_fixture_routes_serve_byte_ranges_or_deliberately_ignore_them
    fixture_routes({ path: '/range', file: 'tool.tar.gz', ranges: 'honor' })
    response = request('GET', '/range', 'Range' => 'bytes=2-5')
    assert_includes response, '206 Partial Content'
    assert_includes response, 'Content-Range: bytes 2-5/14'
    assert response.end_with?('tifa')
    assert_includes request('GET', '/range', 'Range' => 'bytes=20-25'), '416 Range Not Satisfiable'
    fixture_routes({ path: '/range', file: 'tool.tar.gz', ranges: 'ignore' })
    response = request('GET', '/range', 'Range' => 'bytes=2-5')
    assert_includes response, '200 OK'
    assert response.end_with?('artifact bytes')
  end

  def test_fixture_routes_support_unknown_length_responses
    fixture_routes({ path: '/stream', body: 'stream bytes', omit_length: true })
    response = request('GET', '/stream')
    refute_includes response, 'Content-Length:'
    assert response.end_with?('stream bytes')
  end
end
