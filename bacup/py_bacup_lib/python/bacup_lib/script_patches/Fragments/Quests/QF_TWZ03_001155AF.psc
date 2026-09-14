Function CompleteIfAllTargetsPlaced()
	If IsStageDone(11) && IsStageDone(12) && IsStageDone(13) && IsStageDone(14) && IsStageDone(15)
		SetStage(1000)
	EndIf
EndFunction

Function PlaceTarget(ReferenceAlias targetAlias, ObjectReference paperTarget, Int objectiveIndex)
	TWZ03_TargetStandScript targetStand = targetAlias.GetReference() as TWZ03_TargetStandScript
	If targetStand && paperTarget
		targetStand.EnableTargetClient(paperTarget)
	EndIf
	SetObjectiveCompleted(objectiveIndex)
	CompleteIfAllTargetsPlaced()
EndFunction

Function Fragment_Stage_0011_Item_00()
	PlaceTarget(Alias_Target01, TWZ03PaperTarget01Ref, 11)
EndFunction

Function Fragment_Stage_0012_Item_00()
	PlaceTarget(Alias_Target02, TWZ03PaperTarget02Ref, 12)
EndFunction

Function Fragment_Stage_0013_Item_00()
	PlaceTarget(Alias_Target03, TWZ03PaperTarget03Ref, 13)
EndFunction

Function Fragment_Stage_0014_Item_00()
	PlaceTarget(Alias_Target04, TWZ03PaperTarget04Ref, 14)
EndFunction

Function Fragment_Stage_0015_Item_00()
	PlaceTarget(Alias_Target05, TWZ03PaperTarget05Ref, 15)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100)
	If TWZ03_Intro
		TWZ03_Intro.Start()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200)
	SetObjectiveDisplayed(300)
	SetObjectiveDisplayed(11)
	SetObjectiveDisplayed(12)
	SetObjectiveDisplayed(13)
	SetObjectiveDisplayed(14)
	SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(300)
EndFunction
