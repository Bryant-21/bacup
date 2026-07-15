#version 330 core

// Standard mesh attributes
in vec3 in_position;
in vec3 in_normal;
in vec2 in_uv;

// Skinning attributes
in vec4 in_bone_weights;   // 4 bone weights
in ivec4 in_bone_indices;  // 4 bone indices

// Uniforms
uniform mat4 u_mvp;
uniform mat4 u_model;
uniform mat3 u_normal_matrix;
uniform mat4 u_bone_matrices[128];  // Max 128 bones
uniform bool u_skinned;             // Toggle skinning on/off

out vec3 v_normal;
out vec2 v_uv;
out vec3 v_world_pos;

void main() {
    vec4 pos = vec4(in_position, 1.0);
    vec3 norm = in_normal;

    if (u_skinned) {
        mat4 skin_matrix = mat4(0.0);
        for (int i = 0; i < 4; i++) {
            if (in_bone_weights[i] > 0.0) {
                skin_matrix += u_bone_matrices[in_bone_indices[i]] * in_bone_weights[i];
            }
        }
        pos = skin_matrix * pos;
        norm = mat3(skin_matrix) * norm;
    }

    v_world_pos = (u_model * pos).xyz;
    v_normal = normalize(u_normal_matrix * norm);
    v_uv = in_uv;
    gl_Position = u_mvp * pos;
}
