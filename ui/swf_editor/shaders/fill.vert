#version 330 core
uniform mat4 u_projection;
in vec2 in_position;
in vec4 in_color;
out vec4 v_color;
void main() {
    gl_Position = u_projection * vec4(in_position, 0.0, 1.0);
    v_color = in_color;
}
