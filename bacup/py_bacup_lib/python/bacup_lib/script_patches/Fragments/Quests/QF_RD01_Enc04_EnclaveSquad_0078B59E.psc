; Stage behaviour lives in Raids:RD01:Enc04:QuestScript.OnStageSet (stage 100 has no
; fragment slot in the VMAD). This member exists so the VMAD's fragment call resolves
; instead of logging a missing-function error. The fragment VMAD's Alias_Actor_Boss and
; RD01_Enc01_* properties are stale FO76 leftovers and stay unused.
Function Fragment_Stage_9000_Item_00()
EndFunction
