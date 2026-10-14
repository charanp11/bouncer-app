//! Claude Code's questions (`AskUserQuestion`) and plans (`ExitPlanMode`):
//! which ones Bouncer can answer, and the answer it sends.
//!
//! Shared by the relay (which builds `updatedInput` from its own copy of the
//! tool input) and the app (which decides whether a card is shown). The app
//! only ever sends option numbers, so no text from the page reaches Claude
//! Code. Anything this module doesn't accept gets no card and no answer: the
//! terminal asks, as it would without Bouncer.

use serde_json::{Map, Value};

/// The tool that asks the user to pick options.
pub const ASK: &str = "AskUserQuestion";
/// The tool that asks the user to approve a plan.
pub const PLAN: &str = "ExitPlanMode";

/// Claude Code's own limits for `AskUserQuestion` (1–4 questions, 2–4
/// options each); anything outside them is left to the terminal.
const QUESTIONS: std::ops::RangeInclusive<usize> = 1..=4;
const OPTIONS: std::ops::RangeInclusive<usize> = 2..=4;
/// How Claude Code joins the labels of a multi-select answer.
const JOIN: &str = ", ";

pub struct Question<'a> {
    pub text: &'a str,
    pub header: Option<&'a str>,
    pub multi: bool,
    /// Each option's label and description.
    pub options: Vec<(&'a str, Option<&'a str>)>,
}

/// The questions of an `AskUserQuestion` input, if every one can be answered
/// unambiguously by picking options: 1–4 questions with unique, non-empty
/// texts; 2–4 options each with unique, non-empty labels; `multiSelect` a
/// bool or absent; no ", " in a multi-select's labels (the answer joins them
/// with it); no `answers` already. Otherwise `None`.
pub fn questions(input: &Value) -> Option<Vec<Question<'_>>> {
    let input = input.as_object()?;
    if input.contains_key("answers") {
        return None;
    }
    let list = input.get("questions")?.as_array()?;
    if !QUESTIONS.contains(&list.len()) {
        return None;
    }
    let mut out: Vec<Question> = Vec::new();
    for q in list {
        let text = q.get("question")?.as_str()?;
        let header = match q.get("header") {
            None => None,
            Some(h) => Some(h.as_str()?),
        };
        let multi = match q.get("multiSelect") {
            None => false,
            Some(m) => m.as_bool()?,
        };
        let options = q.get("options")?.as_array()?;
        if text.is_empty()
            || out.iter().any(|o| o.text == text)
            || !OPTIONS.contains(&options.len())
        {
            return None;
        }
        let mut labels: Vec<(&str, Option<&str>)> = Vec::new();
        for o in options {
            let label = o.get("label")?.as_str()?;
            let description = match o.get("description") {
                None => None,
                Some(d) => Some(d.as_str()?),
            };
            if label.is_empty()
                || labels.iter().any(|(l, _)| *l == label)
                || (multi && label.contains(JOIN))
            {
                return None;
            }
            labels.push((label, description));
        }
        out.push(Question {
            text,
            header,
            multi,
            options: labels,
        });
    }
    Some(out)
}

/// Parses the app's picks for `questions`: one group per question, separated
/// by `;`, each the chosen option numbers (0-based, one digit) separated by
/// `,`, strictly increasing; exactly one for a single-select question. Any
/// other text is `None`.
pub fn picks(questions: &[Question], text: &str) -> Option<Vec<Vec<usize>>> {
    let groups: Vec<&str> = text.split(';').collect();
    if groups.len() != questions.len() {
        return None;
    }
    let mut out = Vec::new();
    for (q, group) in questions.iter().zip(groups) {
        let mut chosen = Vec::new();
        for n in group.split(',') {
            let [d @ b'0'..=b'9'] = n.as_bytes() else {
                return None;
            };
            let i = usize::from(d - b'0');
            if i >= q.options.len() || chosen.last().is_some_and(|&last| last >= i) {
                return None;
            }
            chosen.push(i);
        }
        if !q.multi && chosen.len() != 1 {
            return None;
        }
        out.push(chosen);
    }
    Some(out)
}

/// The picks line for the wire (`0;1,2`): the inverse of [`picks`].
pub fn picks_line(picks: &[Vec<usize>]) -> String {
    picks
        .iter()
        .map(|g| g.iter().map(usize::to_string).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join(";")
}

/// The `updatedInput` that answers an `AskUserQuestion`: the input exactly as
/// Claude Code sent it, plus `answers` (question text → chosen labels).
/// `None` unless the input and the picks are both accepted.
pub fn answered(input: &Value, picks_text: &str) -> Option<Value> {
    let qs = questions(input)?;
    let chosen = picks(&qs, picks_text)?;
    let mut answers = Map::new();
    for (q, group) in qs.iter().zip(&chosen) {
        let labels: Vec<&str> = group.iter().map(|&i| q.options[i].0).collect();
        answers.insert(q.text.to_owned(), Value::from(labels.join(JOIN)));
    }
    let mut out = input.clone();
    out.as_object_mut()?
        .insert("answers".into(), Value::Object(answers));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ask(multi: bool) -> Value {
        json!({ "questions": [
            { "question": "Which color?", "header": "Color", "multiSelect": multi, "options": [
                { "label": "Red", "description": "Warm" }, { "label": "Blue" }, { "label": "Green" } ] },
            { "question": "Which size?", "options": [ { "label": "Small" }, { "label": "Large" } ] },
        ]})
    }

    #[test]
    fn answers_echo_the_input_and_add_the_chosen_labels() {
        let out = answered(&ask(false), "1;0").unwrap();
        let mut want = ask(false);
        want["answers"] = json!({ "Which color?": "Blue", "Which size?": "Small" });
        assert_eq!(out, want);
        let out = answered(&ask(true), "0,2;1").unwrap();
        assert_eq!(
            out["answers"],
            json!({ "Which color?": "Red, Green", "Which size?": "Large" })
        );
        assert_eq!(out["questions"], ask(true)["questions"]);
    }

    #[test]
    fn picks_must_cover_every_question_once_and_in_range() {
        let single = ask(false);
        for bad in [
            "", "1", "1;", ";0", "1;0;0", "3;0", "1;2", "0,1;0", "1,1;0", "-1;0", "01;0", "a;0",
            " 1;0", "1;0 ", "1;0\n", "+1;0", "１;0", "10;0",
        ] {
            assert!(answered(&single, bad).is_none(), "{bad:?}");
        }
        let multi = ask(true);
        for bad in ["2,0;0", "0,0;1", "0,;1", ",0;1", "0,1,2,3;0"] {
            assert!(answered(&multi, bad).is_none(), "{bad:?}");
        }
        assert!(answered(&multi, "0,1,2;0").is_some());
    }

    #[test]
    fn only_unambiguous_questions_are_accepted() {
        let ok = ask(true);
        assert!(questions(&ok).is_some());
        let changed = |f: &dyn Fn(&mut Value)| {
            let mut v = ok.clone();
            f(&mut v);
            questions(&v).is_none()
        };
        assert!(changed(&|v| v["answers"] = json!({})));
        assert!(changed(&|v| v["questions"] = json!([])));
        assert!(changed(&|v| v["questions"] = json!("x")));
        assert!(changed(&|v| {
            let q = v["questions"][1].clone();
            for _ in 0..3 {
                v["questions"].as_array_mut().unwrap().push(q.clone());
            }
        }));
        assert!(changed(
            &|v| v["questions"][1]["question"] = json!("Which color?")
        ));
        assert!(changed(&|v| v["questions"][0]["question"] = json!("")));
        assert!(changed(&|v| v["questions"][0]["question"] = json!(7)));
        assert!(changed(&|v| v["questions"][0]["header"] = json!(null)));
        assert!(changed(
            &|v| v["questions"][0]["multiSelect"] = json!("true")
        ));
        assert!(changed(
            &|v| v["questions"][0]["options"] = json!([{ "label": "Only" }])
        ));
        assert!(changed(
            &|v| v["questions"][0]["options"][1]["label"] = json!("Red")
        ));
        assert!(changed(
            &|v| v["questions"][0]["options"][1]["label"] = json!("")
        ));
        assert!(changed(
            &|v| v["questions"][0]["options"][1]["description"] = json!(1)
        ));
        assert!(changed(
            &|v| v["questions"][0]["options"][0]["label"] = json!("Red, dark")
        ));
        assert!(changed(&|v| {
            v["questions"][0]["options"]
                .as_array_mut()
                .unwrap()
                .extend([json!({ "label": "D" }), json!({ "label": "E" })]);
        }));
        assert!(questions(&json!([])).is_none());
        // A single-select label may contain ", ": its answer is that one label.
        let mut single = ask(false);
        single["questions"][0]["options"][0]["label"] = json!("Red, dark");
        assert_eq!(
            answered(&single, "0;1").unwrap()["answers"]["Which color?"],
            "Red, dark"
        );
    }

    #[test]
    fn the_picks_line_round_trips() {
        assert_eq!(picks_line(&[vec![0, 2], vec![1]]), "0,2;1");
        let v = ask(true);
        let parsed = picks(&questions(&v).unwrap(), "0,2;1").unwrap();
        assert_eq!(parsed, vec![vec![0, 2], vec![1]]);
    }
}
