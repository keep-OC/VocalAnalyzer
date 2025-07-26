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

            // âπäKÇ…ëŒâûÇ∑ÇÈîíåÆ/çïåÆÇÃîªï Åi12âπäK, 0 = îí, 1 = çïÅj
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
                /*const float C2 = 65.406f;
                const float C5 = 523.251f;
                return saturate((log2(f / C2)) / (log2(C5 / C2)));*/
                const float E2 = 82.407f;
                const float G5 = 783.991f;
                return saturate((log2(f / E2)) / (log2(G5 / E2)));
            }

            half4 frag(v2f i) : SV_Target
            {
                float2 uv = i.uv;
                float2 srcUV = uv + float2(_PixelX, 0.0);
                half4 col = (srcUV.x <= 1.0) ? tex2D(_GrabPassTexture, srcUV) : half4(0, 0, 0, 1);

                // ==== âπäKårê¸ï\é¶ ====
                const int semitoneStart = 40; // E2
                const int semitoneEnd = 79;   // G5

                for (int midi = semitoneStart; midi <= semitoneEnd; ++midi)
                {
                    float freq = 440.0 * pow(2.0, (midi - 69) / 12.0);
                    float y = freqToY(freq);
                    float dist = abs(uv.y - y);

                    float3 lineColor;
                    float thickness;

                    if (midi == 48 || midi == 60 || midi == 72) { // C3, C4, C5
                        lineColor = float3(1.0, 1.0, 1.0); // ëæîíê¸
                        thickness = 0.002;
                    }
                    else if (!isBlackKey(midi)) {
                        lineColor = float3(0.5, 0.5, 0.5); // îíåÆ
                        thickness = 0.001;
                    }
                    else {
                        lineColor = float3(0.1, 0.1, 0.1); // çïåÆ
                        thickness = 0.001;
                    }

                    if (dist < thickness) {
                        col.rgb = lineColor;
                    }
                }

                // f0 ÇÃïúå≥

                float f0 = 0.0;
                if (_FT_L > 0) f0 += _FT_L * 127;
                if (_FT_H > 0) f0 += (_FT_H * 127) * 128.0;

                // E2Å`G5 ÇÃîÕàÕì‡Ç©ämîF
                if (_G1 > 0.05 && f0 >= 0 && f0 <= 16383 && uv.x > 1.0 - _PixelX * 1.5)
                {
                    // float y = freqToY(f0);
                    float y = (f0 / 16383.0);
                    float dist = abs(uv.y - y);
                    float size = 0.005;
                    float intensity = smoothstep(size, 0.0, dist);
                    col.rgb = lerp(col.rgb, float3(1.0, 1.0, 0.0), intensity); // â©êFÇÃì_
                }

                return col;
            }
            ENDCG
        }
    }
}
