use crate::{Error, Result};
use serde_json::{Value, json};

pub(crate) fn asset_schema() -> Value {
    json!({"type":"object","properties":{"asset_id":{"type":"string","minLength":1},"asset_version_id":{"type":"string","minLength":1}},"required":["asset_id","asset_version_id"],"additionalProperties":false})
}

pub(crate) fn templates() -> Vec<Value> {
    let mut result = Vec::new();
    for (family, model, sizes, maximum, grouped) in [
        ("Seedream 5.0 pro", "doubao-seedream-5-0-pro-260628", vec!["1K","1.5K","2K"], 10, false),
        ("Seedream 5.0 flash", "doubao-seedream-5-0-flash-260915", vec!["1K","1.5K","2K"], 10, false),
        ("Seedream 5.0 lite", "doubao-seedream-5-0-lite-260128", vec!["2K","3K","4K"], 14, true),
        ("Seedream 4.5", "doubao-seedream-4-5-251128", vec!["2K","4K"], 14, true),
        ("Seedream 4.0", "doubao-seedream-4-0-250828", vec!["1K","2K","4K"], 14, true),
    ] {
        let mut fields = vec![json!({"key":"prompt","type":"string","required":true,"description":"生成要求"}),json!({"key":"images","type":"image","required":false,"description":"参考图片"}),json!({"key":"size","type":"enum","required":true,"description":"图像尺寸"}),json!({"key":"watermark","type":"boolean","required":true,"description":"添加水印"})];
        let mut properties = json!({"prompt":{"type":"string","minLength":1},"images":{"type":"array","items":asset_schema(),"maxItems":maximum},"size":{"type":"string","enum":sizes,"default":"2K"},"watermark":{"type":"boolean","default":true}});
        let mut mapping = json!({"prompt":"/prompt","images":"/image","size":"/size","watermark":"/watermark"});
        let mut defaults = json!({"size":"2K","watermark":true,"response_format":"url","stream":false});
        if grouped {
            fields.extend([json!({"key":"sequential_image_generation","type":"enum","required":true,"description":"组图模式"}),json!({"key":"max_images","type":"number","required":true,"description":"最多生成图片数"})]);
            properties["sequential_image_generation"] = json!({"type":"string","enum":["disabled","auto"],"default":"disabled"});
            properties["max_images"] = json!({"type":"integer","minimum":1,"maximum":15,"default":1});
            mapping["sequential_image_generation"] = json!("/sequential_image_generation");
            mapping["max_images"] = json!("/sequential_image_generation_options/max_images");
            defaults["sequential_image_generation"] = json!("disabled");
            defaults["max_images"] = json!(1);
        }
        if !family.ends_with("4.5") && !family.ends_with("4.0") {
            fields.push(json!({"key":"output_format","type":"enum","required":true,"description":"图片格式"}));
            properties["output_format"] = json!({"type":"string","enum":["jpeg","png"],"default":"jpeg"});
            mapping["output_format"] = json!("/output_format");
            defaults["output_format"] = json!("jpeg");
        }
        let required: Vec<_> = fields.iter().filter(|field|field["required"] == true).map(|field|field["key"].clone()).collect();
        result.push(json!({"id":format!("{model}-images-v1"),"name":format!("{family} · 图片生成"),"description":family,"definition":{"type":"online","inputs":fields,"input_schema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"outputs":[{"key":"image","type":"image","description":"生成图片"}],"execution":{"provider":"seedream","operation":"image","connection_id":"","model":model,"capability":model,"defaults":defaults,"input_mapping":mapping,"output_mapping":{"image":"/data"}}}}));
    }
    for model in ["MiniMax-H3", "MiniMax-H3-Highspeed", "MiniMax-H3-Lite", "MiniMax-H3-Max"] {
        for (capability, label, frames) in [("text_to_video","文生视频",vec![]),("first_frame","首帧生视频",vec!["first_frame"]),("last_frame","尾帧生视频",vec!["last_frame"]),("first_last_frame","首尾帧生视频",vec!["first_frame","last_frame"]),("reference","参考素材生视频",vec![])] {
            let framed = !frames.is_empty();
            let ratio = if framed {"adaptive"} else {"16:9"};
            let ratios = if framed {vec!["adaptive"]} else if capability == "text_to_video" || model == "MiniMax-H3-Lite" {vec!["21:9","16:9","4:3","1:1","3:4","9:16"]} else {vec!["adaptive","21:9","16:9","4:3","1:1","3:4","9:16"]};
            let resolutions = if model == "MiniMax-H3" {vec!["768P","2K"]} else {vec!["480P","768P"]};
            let minimum = if model == "MiniMax-H3-Max" {5} else {4};
            let mut fields = vec![json!({"key":"prompt","type":"string","required":true,"description":"生成要求"}),json!({"key":"resolution","type":"enum","required":true,"description":"分辨率"}),json!({"key":"ratio","type":"enum","required":true,"description":"画幅"}),json!({"key":"duration","type":"number","required":true,"description":"生成时长（秒）"})];
            let mut properties = json!({"prompt":{"type":"string","minLength":1,"maxLength":7000},"resolution":{"type":"string","enum":resolutions,"default":"768P"},"ratio":{"type":"string","enum":ratios,"default":ratio},"duration":{"type":"integer","minimum":minimum,"maximum":15,"default":5}});
            let mut mapping = json!({"prompt":"prompt","resolution":"resolution","ratio":"ratio","duration":"duration"});
            let mut defaults = json!({"resolution":"768P","ratio":ratio,"duration":5});
            for frame in frames {
                fields.push(json!({"key":frame,"type":"image","required":true,"description":if frame == "first_frame" {"首帧"} else {"尾帧"}}));
                properties[frame] = asset_schema();
                mapping[frame] = json!(format!("content.{frame}"));
            }
            if capability == "reference" {
                for (role, kind, label, maximum) in [("reference_image","image","参考图片",9),("reference_video","video","参考视频",3),("reference_audio","audio","参考音频",3)] {
                    fields.push(json!({"key":role,"type":kind,"required":false,"description":label}));
                    properties[role] = json!({"type":"array","items":asset_schema(),"maxItems":maximum});
                    mapping[role] = json!(format!("content.{role}"));
                }
            }
            if model == "MiniMax-H3-Max" {
                fields.push(json!({"key":"prompt_expansion_mode","type":"enum","required":true,"description":"提示词扩写"}));
                properties["prompt_expansion_mode"] = json!({"type":"string","enum":["disabled","balanced","quality"],"default":"balanced"});
                mapping["prompt_expansion_mode"] = json!("/extra/prompt_expansion_mode");
                defaults["prompt_expansion_mode"] = json!("balanced");
            }
            let required: Vec<_> = fields.iter().filter(|field|field["required"] == true).map(|field|field["key"].clone()).collect();
            result.push(json!({"id":format!("{model}-{capability}-v2"),"name":format!("{model} · {label}"),"description":"MiniMax 视频 V2","definition":{"type":"online","inputs":fields,"input_schema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"outputs":[{"key":"video","type":"video","description":"生成视频","media_type":"video/mp4"}],"execution":{"provider":"minimax","operation":"video","connection_id":"","model":model,"capability":capability,"defaults":defaults,"input_mapping":mapping,"output_mapping":{"video":"/task/content/url"}}}}));
        }
    }
    result
}

pub(crate) fn validate_minimax(body: &Value, capability: &str) -> Result<()> {
    let model = body["model"].as_str().ok_or(Error::Invalid("选择 MiniMax 模型"))?;
    let resolutions = match model {"MiniMax-H3"=>["768P","2K"],"MiniMax-H3-Highspeed"|"MiniMax-H3-Lite"|"MiniMax-H3-Max"=>["480P","768P"],_=>return Err(Error::Invalid("MiniMax 模型无效"))};
    if !resolutions.contains(&body["resolution"].as_str().unwrap_or("")) {return Err(Error::Invalid("分辨率与 MiniMax 模型不一致"));}
    let minimum = if model == "MiniMax-H3-Max" {5} else {4};
    if !body["duration"].as_i64().is_some_and(|duration|(minimum..=15).contains(&duration)) {return Err(Error::Invalid("MiniMax 生成时长无效"));}
    let content = body["content"].as_array().ok_or(Error::Invalid("MiniMax 内容需要数组"))?;
    let text: Vec<_> = content.iter().filter(|item|item["type"] == "text").collect();
    if text.len()!=1 || !text[0]["text"].as_str().is_some_and(|value|!value.trim().is_empty() && value.chars().count()<=7000) {return Err(Error::Invalid("填写不超过 7000 个字符的生成要求"));}
    let count = |role: &str| content.iter().filter(|item|item["role"] == role).count();
    let first=count("first_frame"); let last=count("last_frame");
    let references=count("reference_image")+count("reference_video")+count("reference_audio");
    if first>1 || last>1 || count("reference_image")>9 || count("reference_video")>3 || count("reference_audio")>3 || (references>0 && first+last>0) {return Err(Error::Invalid("MiniMax 素材组合或数量无效"));}
    let expected = match capability {"text_to_video"=>(0,0),"first_frame"=>(1,0),"last_frame"=>(0,1),"first_last_frame"=>(1,1),"reference"=>(0,0),_=>return Err(Error::Invalid("MiniMax 生成能力无效"))};
    if (first,last)!=expected || (capability=="reference")!=(references>0) {return Err(Error::Invalid("素材与 MiniMax 工作流能力不一致"));}
    let ratio=body["ratio"].as_str().unwrap_or("");
    if !["adaptive","21:9","16:9","4:3","1:1","3:4","9:16"].contains(&ratio) || (ratio=="adaptive" && (capability=="text_to_video" || (capability=="reference" && model=="MiniMax-H3-Lite"))) {return Err(Error::Invalid("MiniMax 画幅无效"));}
    if first+last>0 && ratio!="adaptive" {return Err(Error::Invalid("MiniMax 首尾帧画幅需要 adaptive"));}
    if let Some(extra)=body.get("extra") {
        if model!="MiniMax-H3-Max" || !["disabled","balanced","quality"].contains(&extra["prompt_expansion_mode"].as_str().unwrap_or("")) {return Err(Error::Invalid("MiniMax 提示词扩写参数无效"));}
    }
    if serde_json::to_vec(body)?.len()>64*1024*1024 {return Err(Error::Invalid("MiniMax 请求超过 64 MiB"));}
    Ok(())
}

pub(crate) fn validate_seedream(body: &mut Value, family: &str) -> Result<()> {
    if !body["prompt"].as_str().is_some_and(|prompt|!prompt.trim().is_empty()) {return Err(Error::Invalid("填写图片生成要求"));}
    let grouped = family.contains("5-0-lite") || family.contains("4-5") || family.contains("4-0");
    let maximum = if grouped {14} else {10};
    let images = body["image"].as_array().map(Vec::len).unwrap_or(0);
    if images>maximum {return Err(Error::Invalid("参考图片数量超过所选模型上限"));}
    if images==0 {body.as_object_mut().ok_or(Error::Invalid("图片生成参数需要对象"))?.remove("image");}
    let size=body["size"].as_str().ok_or(Error::Invalid("选择图片尺寸"))?;
    let sizes=if family.contains("5-0-lite") {vec!["2K","3K","4K"]} else if family.contains("4-5") {vec!["2K","4K"]} else if family.contains("4-0") {vec!["1K","2K","4K"]} else {vec!["1K","1.5K","2K"]};
    if !sizes.contains(&size) {return Err(Error::Invalid("图片尺寸与模型系列不一致"));}
    if !body["watermark"].is_boolean() || body["stream"]!=false || !["url","b64_json"].contains(&body["response_format"].as_str().unwrap_or("")) {return Err(Error::Invalid("图片生成返回格式或水印参数无效"));}
    if grouped {
        if !["auto","disabled"].contains(&body["sequential_image_generation"].as_str().unwrap_or("")) {return Err(Error::Invalid("组图模式无效"));}
        let maximum=body["sequential_image_generation_options"]["max_images"].as_u64().ok_or(Error::Invalid("填写最多生成图片数"))?;
        if maximum<1 || maximum+images as u64>15 {return Err(Error::Invalid("参考图片与生成图片数量合计不能超过 15 张"));}
    } else {
        let object=body.as_object_mut().ok_or(Error::Invalid("图片生成参数需要对象"))?;
        object.remove("sequential_image_generation");
        object.remove("sequential_image_generation_options");
    }
    if family.contains("4-5") || family.contains("4-0") {body.as_object_mut().ok_or(Error::Invalid("图片生成参数需要对象"))?.remove("output_format");}
    else if !["jpeg","png"].contains(&body["output_format"].as_str().unwrap_or("")) {return Err(Error::Invalid("图片文件格式无效"));}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimax_validates_model_limits_and_media_combinations() {
        let mut body=json!({"model":"MiniMax-H3-Max","content":[{"type":"text","text":"山间清晨"}],"resolution":"768P","duration":5,"ratio":"16:9"});
        assert!(validate_minimax(&body,"text_to_video").is_ok());
        body["duration"]=json!(4);
        assert!(validate_minimax(&body,"text_to_video").is_err());
        body["duration"]=json!(5);
        body["content"].as_array_mut().expect("内容缺失").push(json!({"type":"image_url","role":"first_frame"}));
        assert!(validate_minimax(&body,"text_to_video").is_err());
        body["ratio"]=json!("adaptive");
        assert!(validate_minimax(&body,"first_frame").is_ok());
        body["content"].as_array_mut().expect("内容缺失").push(json!({"type":"audio_url","role":"reference_audio"}));
        assert!(validate_minimax(&body,"first_frame").is_err());
    }

    #[test]
    fn seedream_preserves_model_specific_parameters() {
        let mut body=json!({"prompt":"山间清晨","image":[],"size":"2K","watermark":true,"response_format":"url","stream":false,"output_format":"jpeg","sequential_image_generation":"auto","sequential_image_generation_options":{"max_images":16}});
        assert!(validate_seedream(&mut body.clone(),"doubao-seedream-4-5-251128").is_err());
        body["sequential_image_generation_options"]["max_images"]=json!(2);
        assert!(validate_seedream(&mut body,"doubao-seedream-4-5-251128").is_ok());
        assert!(body.get("image").is_none() && body.get("output_format").is_none());
        assert_eq!(body["sequential_image_generation_options"]["max_images"],2);
    }
}
