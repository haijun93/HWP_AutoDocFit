use super::*;
use std::time::Duration;
fn query(text:&str)->Query{Query{id:"q1".into(),kind:"sample".into(),text:text.into(),filter:None,comparability:"comparable".into(),expected_scope:"filename".into()}}
#[test]fn stats_empty(){assert_eq!(statistics(&[]).count,0);assert_eq!(statistics(&[]).mean,None);}
#[test]fn stats_single(){let s=statistics(&[7.]);assert_eq!(s.p50,Some(7.));assert_eq!(s.stddev,Some(0.));assert_eq!(s.min,Some(7.));}
#[test]fn stats_exact_values(){let s=statistics(&[1.,2.,3.,4.,5.]);assert_eq!(s.p50,Some(3.));assert_eq!(s.mean,Some(3.));assert!((s.stddev.unwrap()-2f64.sqrt()).abs()<1e-12);}
#[test]fn stats_percentiles(){let s=statistics(&[1.,2.,3.,4.,5.]);assert_eq!(s.p90,Some(4.6));assert_eq!(s.p95,Some(4.8));assert_eq!(s.p99,Some(4.96));}
#[test]fn outliers_flagged_not_removed(){let s=statistics(&[1.,1.,1.,1.,100.]);assert_eq!(s.outlier_count,1);assert_eq!(s.count,5);}
#[test]fn low_sample_warning(){assert!(statistics(&[1.;9]).low_sample_warning);assert!(!statistics(&[1.;10]).low_sample_warning);}
#[test]fn sha256_known(){assert_eq!(sha256(b"abc"),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");}
#[test]fn digest_hides_text(){let d=digest(&query("private"),false);assert_eq!(d.query_text,None);assert_eq!(d.query_length,7);}
#[test]fn digest_opt_in(){assert_eq!(digest(&query("abc"),true).query_text,Some("abc".into()));}
#[test]fn query_validation(){let q=QuerySet{schema_version:1,set_version:"v1".into(),seed:None,queries:vec![query("hi")]};assert!(validate_query_set(&q).is_ok());}
#[test]fn query_reject_schema(){let mut q=QuerySet{schema_version:2,set_version:"v1".into(),seed:None,queries:vec![]};assert!(validate_query_set(&q).is_err());q.schema_version=1;}
#[test]fn query_reject_duplicate_id(){let q=query("a");let mut r=query("b");r.id=q.id.clone();assert!(validate_query_set(&QuerySet{schema_version:1,set_version:"v".into(),seed:None,queries:vec![q,r]}).is_err());}
#[test]fn query_reject_bad_kind_class(){let mut q=query("x");q.comparability="maybe".into();assert!(validate_query_set(&QuerySet{schema_version:1,set_version:"v".into(),seed:None,queries:vec![q]}).is_err());}
#[test]fn deterministic_sampler(){let n=vec!["/a/alpha.txt".into(),"/b/beta.pdf".into(),"/c/gamma.doc".into()];assert_eq!(sample_queries(&n,44,2),sample_queries(&n,44,2));}
#[test]fn sampler_respects_count(){let n=vec!["a.txt".into(),"b.txt".into(),"c.txt".into()];assert_eq!(sample_queries(&n,1,2).queries.len(),2);}
#[test]fn sampler_dedupes(){let n=vec!["/a/same.txt".into(),"/b/same.pdf".into()];assert_eq!(sample_queries(&n,1,5).queries.len(),1);}
#[test]fn sampler_stores_seed(){assert_eq!(sample_queries(&[],9,5).seed,Some(9));}
#[test]fn fake_top_n_and_zero_count(){let mut a=FakeAdapter{name:"fake".into(),results:vec![],latency_ms:2.,error:None,timed_out:false};let r=a.query(&query("z"),3,Duration::from_secs(1));assert_eq!(r.result_count,0);assert!(r.top_ids.is_empty());}
#[test]fn fake_adapter_top_n_hashes(){let mut a=FakeAdapter{name:"fake".into(),results:vec!["a".into(),"b".into()],latency_ms:2.,error:None,timed_out:false};let r=a.query(&query("z"),1,Duration::from_secs(1));assert_eq!(r.top_ids,vec![sha256(b"a")]);}
struct FakeClock(u64);impl Clock for FakeClock{fn now_ms(&mut self)->u64{self.0+=5;self.0}}
#[test]fn measure_warmup_separated(){let mut a=FakeAdapter{name:"f".into(),results:vec![],latency_ms:1.,error:None,timed_out:false};let mut c=FakeClock(0);let r=measure(&mut a,&mut c,&NeverCancel,&[query("a")],2,3,0,4,Duration::from_secs(1));assert_eq!(r.observations.iter().filter(|x|x.phase=="warmup").count(),2);assert_eq!(r.observations.iter().filter(|x|x.phase=="warm").count(),3);assert!(r.observations.iter().all(|x|x.phase!="cold"));}
#[test]fn measure_timeout_status(){let mut a=FakeAdapter{name:"f".into(),results:vec![],latency_ms:12.,error:None,timed_out:true};let r=measure(&mut a,&mut FakeClock(0),&NeverCancel,&[query("a")],0,1,0,4,Duration::from_millis(2));assert_eq!(r.observations[0].status,"timeout");}
#[test]fn measure_error_status(){let mut a=FakeAdapter{name:"f".into(),results:vec![],latency_ms:1.,error:Some("nope".into()),timed_out:false};let r=measure(&mut a,&mut FakeClock(0),&NeverCancel,&[query("a")],0,1,0,4,Duration::from_secs(1));assert_eq!(r.observations[0].status,"error");}
struct CancelAt(usize);impl Cancellation for CancelAt{fn is_cancelled(&self)->bool{self.0==1}}
#[test]fn measure_cancel_signal(){let mut a=FakeAdapter{name:"f".into(),results:vec![],latency_ms:1.,error:None,timed_out:false};let r=measure(&mut a,&mut FakeClock(0),&CancelAt(1),&[query("a")],0,3,0,4,Duration::from_secs(1));assert!(r.cancelled);assert!(r.observations.is_empty());}
#[test]fn measure_keeps_result_count(){let mut a=FakeAdapter{name:"f".into(),results:vec!["x".into(),"y".into()],latency_ms:1.,error:None,timed_out:false};let r=measure(&mut a,&mut FakeClock(0),&NeverCancel,&[query("a")],0,1,0,4,Duration::from_secs(1));assert_eq!(r.observations[0].result_count,2);}
fn sample(t:u64,ws:u64,pr:u64,cpu:u64,io:u64)->ResourceSample{ResourceSample{at_ms:t,working_set_bytes:Some(ws),private_bytes:Some(pr),cpu_time_ms:Some(cpu),read_bytes:Some(io),write_bytes:Some(io*2),read_ops:Some(io/2),write_ops:Some(io/3),handles:Some(io),threads:Some(2)}}
#[test]fn resource_delta_and_peaks(){let s=summarize_samples(&[sample(0,10,20,4,3),sample(1,15,29,9,8)],false);assert_eq!(s.working_set_peak_bytes,Some(15));assert_eq!(s.private_delta_bytes,Some(9));assert_eq!(s.cpu_delta_ms,Some(5));assert_eq!(s.read_bytes_delta,Some(5));assert_eq!(s.write_bytes_delta,Some(10));}
#[test]fn idle_flag_separate(){let s=summarize_samples(&[sample(0,1,2,3,4)],true);assert!(s.idle_baseline);}
#[test]fn resource_empty_reason(){let s=summarize_samples(&[],false);assert_eq!(s.status,"unavailable");assert_eq!(s.unavailable_reason.as_deref(),Some("no_samples"));}
#[test]fn result_schema_required_fields(){let v=serde_json::json!({"schema_version":1,"run":{},"queries":[],"resources":{},"summary":{}});assert!(validate_result(&v).is_ok());}
#[test]fn result_schema_missing_field(){assert!(validate_result(&serde_json::json!({"schema_version":1})).is_err());}
#[test]fn csv_redacts_query(){let d=digest(&query("secret text"),false);let csv=csv_summary("r",&[(d,statistics(&[]),0,0,0)]);assert!(!csv.contains("secret text"));assert!(csv.contains("query_hash"));}
