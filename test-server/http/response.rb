# frozen_string_literal: true

module LocalArtifactServer
  STATUS_TEXT = {
    200 => 'OK',
    400 => 'Bad Request',
    404 => 'Not Found',
    405 => 'Method Not Allowed',
    500 => 'Internal Server Error'
  }.freeze

  Response = Struct.new(:status, :body, :content_type, :content_length, :stream_writer, keyword_init: true) do
    def write(socket, request = nil)
      body = self.body.to_s.b
      write_headers(socket, body.bytesize)
      return if request&.method == 'HEAD'

      stream_writer ? stream_writer.call(socket) : socket.write(body)
    end

    private

    def write_headers(socket, body_length)
      headers = [
        "HTTP/1.1 #{status} #{LocalArtifactServer::STATUS_TEXT.fetch(status, 'Unknown')}",
        "Content-Type: #{content_type}",
        "Content-Length: #{content_length || body_length}",
        'Connection: close',
        "\r\n"
      ]
      socket.write(headers.join("\r\n"))
    end
  end
end
