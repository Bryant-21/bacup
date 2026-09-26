Function CompleteIfAllTargetsPlaced()
	TWZ03_Script controller = (Self as Quest) as TWZ03_Script
	If controller != None
		controller.CompleteIfAllTargetsPlaced()
	EndIf
EndFunction

Function ResetPaperTargets()
	DisablePaperTarget(TWZ03PaperTarget01Ref)
	DisablePaperTarget(TWZ03PaperTarget02Ref)
	DisablePaperTarget(TWZ03PaperTarget03Ref)
	DisablePaperTarget(TWZ03PaperTarget04Ref)
	DisablePaperTarget(TWZ03PaperTarget05Ref)
EndFunction

Function DisablePaperTarget(ObjectReference paperTarget)
	If paperTarget != None && paperTarget.IsEnabled()
		paperTarget.Disable(False)
	EndIf
EndFunction

Function PlaceTarget(ReferenceAlias targetAlias, ObjectReference paperTarget)
	TWZ03_TargetStandScript targetStand = None
	If targetAlias != None
		targetStand = targetAlias.GetReference() as TWZ03_TargetStandScript
	EndIf
	If targetStand != None && paperTarget != None
		targetStand.EnableTargetClient(paperTarget)
	EndIf
	; Objective 300's five targets are each conditioned on GetStageDone(11..15) == 0 and the
	; quest does not carry RepeatsConditions, so the marker set only refreshes on a forced redisplay.
	If !IsStageDone(1000)
		SetObjectiveDisplayed(300, True, True)
	EndIf
	CompleteIfAllTargetsPlaced()
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	ReferenceAlias attendantAlias = GetAlias(1) as ReferenceAlias
	If attendantAlias == None || akSender != attendantAlias.GetReference()
		Return
	EndIf
	If akActionRef != Game.GetPlayer() || !IsStageDone(100) || IsStageDone(200)
		Return
	EndIf
	SetStage(200)
EndEvent

Function Fragment_Stage_0011_Item_00()
	PlaceTarget(Alias_Target01, TWZ03PaperTarget01Ref)
EndFunction

Function Fragment_Stage_0012_Item_00()
	PlaceTarget(Alias_Target02, TWZ03PaperTarget02Ref)
EndFunction

Function Fragment_Stage_0013_Item_00()
	PlaceTarget(Alias_Target03, TWZ03PaperTarget03Ref)
EndFunction

Function Fragment_Stage_0014_Item_00()
	PlaceTarget(Alias_Target04, TWZ03PaperTarget04Ref)
EndFunction

Function Fragment_Stage_0015_Item_00()
	PlaceTarget(Alias_Target05, TWZ03PaperTarget05Ref)
EndFunction

Function Fragment_Stage_0100_Item_00()
	ResetPaperTargets()

	; Stage 200 was set by the attendant's FO76 dialogue, which does not survive conversion.
	ReferenceAlias attendantAlias = GetAlias(1) as ReferenceAlias
	If attendantAlias != None && attendantAlias.GetReference() != None
		RegisterForRemoteEvent(attendantAlias.GetReference(), "OnActivate")
	EndIf

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
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(300)
	ReferenceAlias attendantAlias = GetAlias(1) as ReferenceAlias
	If attendantAlias != None && attendantAlias.GetReference() != None
		UnregisterForRemoteEvent(attendantAlias.GetReference(), "OnActivate")
	EndIf
EndFunction
