# frozen_string_literal: true

module LocalArtifactServer
  STATUS_TEXT = {
    200 => 'OK',
    206 => 'Partial Content',
    400 => 'Bad Request',
    404 => 'Not Found',
    405 => 'Method Not Allowed',
    416 => 'Range Not Satisfiable',
    500 => 'Internal Server Error'
  }.freeze

  Response = Struct.new(:status, :body, :content_type, :content_length, :stream_writer, :headers, :omit_length, keyword_init: true) do
    def write(socket, request = nil)
      body = self.body.to_s.b
      write_headers(socket, body.bytesize)
      return if request&.method == 'HEAD'

      stream_writer ? stream_writer.call(socket) : socket.write(body)
    end

    private

    def write_headers(socket, body_length)
      lines = [
        "HTTP/1.1 #{status} #{LocalArtifactServer::STATUS_TEXT.fetch(status, 'Unknown')}",
        "Content-Type: #{content_type}"
      ]
      lines << "Content-Length: #{content_length || body_length}" unless omit_length
      (headers || {}).each { |name, value| lines << "#{name}: #{value}" }
      lines.concat(['Connection: close', "\r\n"])
      socket.write(lines.join("\r\n"))
    end
  end
end
