#version 330 core

in vec3 v_normal;
in vec2 v_uv;
in vec3 v_world_pos;

uniform vec3 u_light_dir;
uniform vec3 u_light_color;
uniform vec3 u_ambient;
uniform sampler2D u_diffuse_tex;
uniform bool u_has_texture;

out vec4 frag_color;

void main() {
    vec3 N = normalize(v_normal);
    vec3 L = normalize(u_light_dir);
    float diff = max(dot(N, L), 0.0);

    vec3 base_color;
    if (u_has_texture) {
        base_color = texture(u_diffuse_tex, v_uv).rgb;
    } else {
        base_color = vec3(0.7, 0.7, 0.7);
    }

    vec3 color = base_color * (u_ambient + u_light_color * diff);
    frag_color = vec4(color, 1.0);
}
