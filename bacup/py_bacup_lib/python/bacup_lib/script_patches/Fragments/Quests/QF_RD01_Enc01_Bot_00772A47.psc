; Phase stages 200/300 may come from the SetStageOnHealthThreshold alias script or from Tales;
; the [TEMP] phase messages are placeholders and are not shown.
Function Fragment_Stage_0100_Item_00()
	Raids:RD01:Enc01:QuestScript encounter = Enc01Script()
	If encounter != None
		encounter.StartEncounter()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetBossPhase(2)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetBossPhase(3)
EndFunction

Function Fragment_Stage_9000_Item_00()
	Raids:RD01:Enc01:QuestScript encounter = Enc01Script()
	If encounter != None
		encounter.EncounterVictory()
	EndIf
EndFunction

Function SetBossPhase(Int aiPhase)
	Actor boss = Alias_Actor_Boss.GetActorReference()
	If boss != None && RD01_Enc01_Phase_AV != None
		boss.SetValue(RD01_Enc01_Phase_AV, aiPhase as Float)
	EndIf
	Raids:RD01:Enc01:QuestScript encounter = Enc01Script()
	If encounter != None
		encounter.SetPhase(aiPhase)
	EndIf
EndFunction

Raids:RD01:Enc01:QuestScript Function Enc01Script()
	Return (Self as Quest) as Raids:RD01:Enc01:QuestScript
EndFunction
