Shader "Unlit/GrabPassShader"
{
    Properties
    {
        // _MainTex ("Texture", 2D) = "white" {}
        _PixelX("1/Width", Float) = 0.001953125 // 1.0 / 512.0
        _FT_L("FT_L",     Float) = 0
        _FT_H("FT_H",     Float) = 0
        _G1("G1",       Float) = 0
        _G2("G2",       Float) = 0
        _G3("G3",       Float) = 0
        _G4("G4",       Float) = 0
        _G5("G5",       Float) = 0
        _G6("G6",       Float) = 0
        _G7("G7",       Float) = 0
        _G8("G8",       Float) = 0
        _G9("G9",       Float) = 0
        _G10("G10",       Float) = 0
        _G11("G11",       Float) = 0
        _G12("G12",       Float) = 0
        _G13("G13",       Float) = 0
        _G14("G14",       Float) = 0
        _G15("G15",       Float) = 0
        _G16("G16",       Float) = 0
        _G17("G17",       Float) = 0
        _G18("G18",       Float) = 0
        _G19("G19",       Float) = 0
        _G20("G20",       Float) = 0
    }

    SubShader
    {
        // Tags { "RenderType" = "Opaque" }
        GrabPass { "_GrabPassTexture" }

        Pass
        {
            CGPROGRAM
            #pragma vertex vert
            #pragma fragment frag
            // make fog work
            // #pragma multi_compile_fog

            #include "UnityCG.cginc"

            struct v2f
            {
                float2 uv : TEXCOORD0;
                // UNITY_FOG_COORDS(1)
                float4 vertex : SV_POSITION;
            };

            float _PixelX;
            float _FT_L, _FT_H;
            float _G1, _G2, _G3, _G4, _G5, _G6, _G7, _G8, _G9, _G10;
            float _G11, _G12, _G13, _G14, _G15, _G16, _G17, _G18, _G19, _G20;

            

            sampler2D _GrabPassTexture;

            float3 Heatmap(float v)
            {
                v = saturate(v);
                float3 c1 = float3(0.0, 0.0, 0.0);
                float3 c2 = float3(0.0, 0.8, 1.0);
                float3 c3 = float3(1.0, 1.0, 0.0); 
                float3 c4 = float3(1.0, 0.0, 0.0);

                float t = v * 3.0;
                return (t < 1.0) ? lerp(c1, c2, t)
                    : (t < 2.0) ? lerp(c2, c3, t - 1.0)
                    : lerp(c3, c4, t - 2.0);
            }

            v2f vert(float4 vertex : POSITION)
            {
                v2f o = (v2f)0;
                o.vertex = UnityObjectToClipPos(vertex);
                o.uv = ComputeGrabScreenPos(o.vertex);
                return o;
            }

            half4 frag(v2f i) : SV_Target
            {
                float MAX_HZ = 8192.0;

                float2 src = i.uv + float2(_PixelX, 0.0);
                half4 ret = (src.x <= 1.0)
                    ? tex2D(_GrabPassTexture, src)
                    : half4(0, 0, 0, 1);

                // === Œrü•`‰æ ===
                float lineWidth = _PixelX * 1.0;
                float lineIntensity = 0.0;

                // 1kHz‚²‚Æ‚Ì‘¾ü
                [unroll]
                for (int k = 1; k <= 8; ++k)
                {
                    float freq = 1000.0 * k;
                    float yLine = freq / MAX_HZ;
                    float dist = abs(i.uv.y - yLine);
                    lineIntensity = max(lineIntensity, smoothstep(lineWidth, 0.0, dist));
                }

                // 500Hz‚²‚Æ‚Ì”–üi1kHzœŠOj
                [unroll]
                for (int k = 1; k <= 16; ++k)
                {
                    float freq = 500.0 * k;
                    if (freq % 1000 == 0) continue; // d•¡‰ñ”ð
                    float yLine = freq / MAX_HZ;
                    float dist = abs(i.uv.y - yLine);
                    lineIntensity = max(lineIntensity, smoothstep(lineWidth, 0.0, dist) * 0.3);
                }

                float3 gridColor = float3(1.0, 1.0, 1.0) * lineIntensity;
                ret.rgb = max(ret.rgb, gridColor);

                if (i.uv.x > 1.0 - _PixelX * 1.5)
                {
                    float f0 = 0;
                    if (_FT_L > 0)
                    {
                        f0 += _FT_L * 127;
                    }

                    if (_FT_H > 0)
                    {
                        f0 += (_FT_H * 127) * 128.0;
                    }

                    const float A = 82.407;
                    const float B = 783.991;
                    const float log2A = 6.373;  // log2(82.407)
                    const float log2B = 9.608;  // log2(783.991)
                    float norm = saturate(f0 / 16383.0);
                    f0 = A * pow(B / A, norm);

                    f0 = max(f0, 1.0);

                    float amp = 0.0;
                    [unroll]
                    for (int h = 1; h <= 20; h++)
                    {
                        float freq = f0 * h;
                        if (freq > MAX_HZ) continue;

                        float yPos = freq / MAX_HZ;
                        float dist = abs(i.uv.y - yPos);

                        float band = smoothstep(_PixelX * 3.0, 0.0, dist);

                        float gain = (h == 1) ? _G1 :
                            (h == 2) ? _G2 :
                            (h == 3) ? _G3 :
                            (h == 4) ? _G4 :
                            (h == 5) ? _G5 :
                            (h == 6) ? _G6 :
                            (h == 7) ? _G7 :
                            (h == 8) ? _G8 :
                            (h == 9) ? _G9 :
                            (h == 10) ? _G10 :
                            (h == 11) ? _G11 :
                            (h == 12) ? _G12 :
                            (h == 13) ? _G13 :
                            (h == 14) ? _G14 :
                            (h == 15) ? _G15 :
                            (h == 16) ? _G16 :
                            (h == 17) ? _G17 :
                            (h == 18) ? _G18 :
                            (h == 19) ? _G19 : _G20;

                        amp += band * gain;
                    }

                    float3 col = Heatmap(saturate(amp));
                    ret = half4(col, 1.0);
                }

                return ret;
            }
            ENDCG
        }
    }
}
