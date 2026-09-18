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

  def request(method, path)
    socket = TCPSocket.new('127.0.0.1', @server.server.addr[1])
    socket.write("#{method} #{path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
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
end
