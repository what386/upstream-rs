#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative 'http/server'

if $PROGRAM_NAME == __FILE__
  host = ENV.fetch('SERVER_HOST', '127.0.0.1')
  port = ENV.fetch('SERVER_PORT', '8000')
  root = File.expand_path('artifacts', __dir__)
  server = LocalArtifactServer::HttpServer.new(host: host, port: port, artifact_root: root)
  trap('INT') { server.stop }
  trap('TERM') { server.stop }
  server.start
end
