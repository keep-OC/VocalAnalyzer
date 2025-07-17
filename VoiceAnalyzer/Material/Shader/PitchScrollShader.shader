Shader "Unlit/PitchScrollShader"
{
    Properties
    {
        _PixelX("1/Width", Float) = 0.001953125
        _G1("G1", Float) = 0
        _FT_L("FT_L", Float) = 0
        _FT_H("FT_H", Float) = 0
    }

        SubShader
    {
        Tags { "RenderType" = "Opaque" }
        GrabPass { "_GrabPassTexture" }

        Pass
        {
            CGPROGRAM
            #pragma vertex vert
            #pragma fragment frag

            #include "UnityCG.cginc"

            struct v2f {
                float4 vertex : SV_POSITION;
                float2 uv : TEXCOORD0;
            };

            float _PixelX;
            float _G1;
            float _FT_L, _FT_H;
            sampler2D _GrabPassTexture;

            // ���K�ɑΉ����锒��/�����̔��ʁi12���K, 0 = ��, 1 = ���j
            int isBlackKey(int semitone)
            {
                int p = semitone % 12;
                return (p == 1 || p == 3 || p == 6 || p == 8 || p == 10) ? 1 : 0;
            }

            v2f vert(float4 vertex : POSITION)
            {
                v2f o;
                o.vertex = UnityObjectToClipPos(vertex);
                o.uv = ComputeGrabScreenPos(o.vertex).xy;
                return o;
            }

            float freqToY(float f)
            {
                const float C2 = 65.406f;
                const float C5 = 523.251f;
                return saturate((log2(f / C2)) / (log2(C5 / C2)));
            }

            half4 frag(v2f i) : SV_Target
            {
                float2 uv = i.uv;
                float2 srcUV = uv + float2(_PixelX, 0.0);
                half4 col = (srcUV.x <= 1.0) ? tex2D(_GrabPassTexture, srcUV) : half4(0, 0, 0, 1);

                // ==== ���K�r���\�� ====
                const int semitoneStart = 36; // C2
                const int semitoneEnd = 72;   // C5

                for (int midi = semitoneStart; midi <= semitoneEnd; ++midi)
                {
                    float freq = 440.0 * pow(2.0, (midi - 69) / 12.0);
                    float y = freqToY(freq);
                    float dist = abs(uv.y - y);

                    int note = midi % 12;

                    float3 lineColor;
                    float thickness;

                    if (midi == 36 || midi == 48 || midi == 60 || midi == 72) {
                        lineColor = float3(1.0, 1.0, 1.0); // C2,3,4,5
                        thickness = 0.003; // 1px��
                    }
                    else if (note == 0 || note == 2 || note == 4 || note == 5 || note == 7 || note == 9 || note == 11) {
                        lineColor = float3(0.5, 0.5, 0.5); // ����
                        thickness = 0.001; // ��0.5px
                    }
                    else {
                        lineColor = float3(0.1, 0.1, 0.1); // ����
                        thickness = 0.001; // ����
                    }

                    // �ւ��� step() ���g���Đ�΂ɐ����o��
                    if (dist < thickness) {
                        col.rgb = lineColor;
                    }
                }

                // f0 �̕���
                float f0 = 0.0;
                if (_FT_L > 0) f0 += 1.0 / _FT_L;
                if (_FT_H > 0) f0 += (1.0 / _FT_H) * 128.0;

                // C2�`C5 �͈͓̔����m�F
                if (_G1 > 0.05 && f0 >= 65.406 && f0 <= 523.251 && uv.x > 1.0 - _PixelX * 1.5)
                {
                    float y = freqToY(f0);
                    float dist = abs(uv.y - y);
                    float size = 0.005;
                    float intensity = smoothstep(size, 0.0, dist);
                    col.rgb = lerp(col.rgb, float3(1.0, 1.0, 0.0), intensity); // ���F�̓_
                }

                return col;
            }
            ENDCG
        }
    }
}
