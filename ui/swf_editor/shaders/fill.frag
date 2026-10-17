#version 330 core
in vec4 v_color;
out vec4 fragColor;
uniform float u_opacity;
void main() {
    fragColor = vec4(v_color.rgb, v_color.a * u_opacity);
}
