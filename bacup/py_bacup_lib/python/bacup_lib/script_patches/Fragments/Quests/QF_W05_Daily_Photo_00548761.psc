Function DisplayPhotoObjective(Int objectiveID)
	If HasObjective(objectiveID) && !IsObjectiveCompleted(objectiveID)
		SetObjectiveDisplayed(objectiveID)
	EndIf
EndFunction

Function CompletePhotoObjective(Int objectiveID, Int groupStage)
	If HasObjective(objectiveID) && !IsObjectiveCompleted(objectiveID)
		SetObjectiveCompleted(objectiveID)
	EndIf
	If !IsStageDone(groupStage)
		SetStage(groupStage)
	EndIf
EndFunction

Function CheckPhotoGroupsComplete()
	If IsStageDone(400) && IsStageDone(500) && !IsStageDone(600)
		SetStage(600)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(110)
	; FO76 picked two random targets server side: one in the settlement (Int
	; 210/220/230) and one nearby (Ext 250/260/270). Nothing else sets them.
	PickPhotoTargets(210, 250)
EndFunction

Function PickPhotoTargets(Int aiFirstIntStage, Int aiFirstExtStage)
	Int intStage = aiFirstIntStage + 10 * Utility.RandomInt(0, 2)
	Int extStage = aiFirstExtStage + 10 * Utility.RandomInt(0, 2)
	If !IsStageDone(intStage)
		SetStage(intStage)
	EndIf
	If !IsStageDone(extStage)
		SetStage(extStage)
	EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
	DisplayPhotoObjective(200)
EndFunction

Function Fragment_Stage_0215_Item_00()
	CompletePhotoObjective(200, 400)
EndFunction

Function Fragment_Stage_0220_Item_00()
	DisplayPhotoObjective(220)
EndFunction

Function Fragment_Stage_0225_Item_00()
	CompletePhotoObjective(220, 400)
EndFunction

Function Fragment_Stage_0230_Item_00()
	DisplayPhotoObjective(230)
EndFunction

Function Fragment_Stage_0235_Item_00()
	CompletePhotoObjective(230, 400)
EndFunction

Function Fragment_Stage_0250_Item_00()
	DisplayPhotoObjective(250)
EndFunction

Function Fragment_Stage_0255_Item_00()
	CompletePhotoObjective(250, 500)
EndFunction

Function Fragment_Stage_0260_Item_00()
	DisplayPhotoObjective(260)
EndFunction

Function Fragment_Stage_0265_Item_00()
	CompletePhotoObjective(260, 500)
EndFunction

Function Fragment_Stage_0270_Item_00()
	DisplayPhotoObjective(270)
EndFunction

Function Fragment_Stage_0275_Item_00()
	CompletePhotoObjective(270, 500)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(110)
	PickPhotoTargets(310, 350)
EndFunction

Function Fragment_Stage_0310_Item_00()
	DisplayPhotoObjective(300)
EndFunction

Function Fragment_Stage_0315_Item_00()
	CompletePhotoObjective(300, 400)
EndFunction

Function Fragment_Stage_0320_Item_00()
	DisplayPhotoObjective(320)
EndFunction

Function Fragment_Stage_0325_Item_00()
	CompletePhotoObjective(320, 400)
EndFunction

Function Fragment_Stage_0330_Item_00()
	DisplayPhotoObjective(330)
EndFunction

Function Fragment_Stage_0335_Item_00()
	CompletePhotoObjective(330, 400)
EndFunction

Function Fragment_Stage_0350_Item_00()
	DisplayPhotoObjective(350)
EndFunction

Function Fragment_Stage_0355_Item_00()
	CompletePhotoObjective(350, 500)
EndFunction

Function Fragment_Stage_0360_Item_00()
	DisplayPhotoObjective(360)
EndFunction

Function Fragment_Stage_0365_Item_00()
	CompletePhotoObjective(360, 500)
EndFunction

Function Fragment_Stage_0370_Item_00()
	DisplayPhotoObjective(370)
EndFunction

Function Fragment_Stage_0375_Item_00()
	CompletePhotoObjective(370, 500)
EndFunction

Function Fragment_Stage_0400_Item_00()
	CheckPhotoGroupsComplete()
EndFunction

Function Fragment_Stage_0500_Item_00()
	CheckPhotoGroupsComplete()
EndFunction

Function Fragment_Stage_0600_Item_00()
	If IsStageDone(200)
		SetObjectiveDisplayed(500)
	ElseIf IsStageDone(300)
		SetObjectiveDisplayed(400)
	EndIf
EndFunction

; Davenport's briefing, the two sell conversations and the report-back all ran
; through player-dialogue scenes whose fragments did not survive the FO76 strip.
; Activating the actor stands in for those conversations.
ReferenceAlias Function DavenportAlias()
	Return GetAlias(1) as ReferenceAlias
EndFunction

ReferenceAlias Function BuyerAlias()
	If IsStageDone(200)
		Return GetAlias(11) as ReferenceAlias
	EndIf
	Return GetAlias(10) as ReferenceAlias
EndFunction

Function EnsureDialogueListeners()
	ReferenceAlias davenport = DavenportAlias()
	If davenport != None && davenport.GetReference() != None
		RegisterForRemoteEvent(davenport.GetReference(), "OnActivate")
	EndIf
	ReferenceAlias buyer = BuyerAlias()
	If buyer != None && buyer.GetReference() != None
		RegisterForRemoteEvent(buyer.GetReference(), "OnActivate")
	EndIf
EndFunction

Event OnQuestInit()
	EnsureDialogueListeners()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	EnsureDialogueListeners()
EndEvent

Event OnQuestShutdown()
	UnregisterForAllEvents()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If !IsRunning() || Alias_Player == None
		Return
	EndIf
	Actor player = Alias_Player.GetActorReference()
	If player == None || akActionRef != player
		Return
	EndIf

	ReferenceAlias davenport = DavenportAlias()
	If davenport != None && akSender == davenport.GetReference()
		If !IsStageDone(200) && !IsStageDone(300)
			If !IsStageDone(110)
				SetStage(110)
			EndIf
			If Utility.RandomInt(0, 1) == 0
				SetStage(200)
			Else
				SetStage(300)
			EndIf
		ElseIf IsStageDone(600) && !IsStageDone(2000)
			If IsStageDone(975)
				SetStage(1100)
			Else
				SetStage(1200)
			EndIf
		EndIf
		Return
	EndIf

	ReferenceAlias buyer = BuyerAlias()
	If buyer != None && akSender == buyer.GetReference() && IsStageDone(600) && !IsStageDone(975)
		SetStage(800)
		SetStage(900)
	EndIf
EndEvent

; Debug stages, the objective lock-in checkpoint and the two FO76 instanced-interior
; markers are bound by VMAD but carry no behavior FO4 can reproduce.
Function Fragment_Stage_0120_Item_00()
EndFunction

Function Fragment_Stage_0121_Item_00()
EndFunction

Function Fragment_Stage_0122_Item_00()
EndFunction

Function Fragment_Stage_0123_Item_00()
EndFunction

Function Fragment_Stage_0124_Item_00()
EndFunction

Function Fragment_Stage_0125_Item_00()
EndFunction

Function Fragment_Stage_0150_Item_00()
EndFunction

Function Fragment_Stage_0240_Item_00()
EndFunction

Function Fragment_Stage_0340_Item_00()
EndFunction

; Caps for 700/800/1200/1300/1500 and the scrip/Treasury Notes for 2000 are paid by
; the converter-attached B21:QuestRewards / B21:CurrencyQuestRewards rows.
Function Fragment_Stage_0700_Item_00()
	SetStage(975)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetStage(975)
EndFunction

Function Fragment_Stage_0900_Item_00()
	GiveSellReputation(Rep_Mod_Add_Huge)
	SetStage(975)
EndFunction

Function Fragment_Stage_0950_Item_00()
	GiveSellReputation(Rep_Mod_Add_Medium)
EndFunction

Function GiveSellReputation(GlobalVariable akAmount)
	Actor player = None
	If Alias_Player != None
		player = Alias_Player.GetActorReference()
	EndIf
	If player == None || akAmount == None
		Return
	EndIf
	; The photos are sold to the settlement that was NOT photographed.
	If IsStageDone(200)
		If Reputation_AV_Foundation != None
			player.ModValue(Reputation_AV_Foundation, akAmount.GetValue())
		EndIf
	ElseIf Reputation_AV_Crater != None
		player.ModValue(Reputation_AV_Crater, akAmount.GetValue())
	EndIf
EndFunction

Function Fragment_Stage_0975_Item_00()
	SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(400, True)
	SetObjectiveCompleted(500, True)
	SetObjectiveDisplayed(600, True, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
	Actor player = None
	If Alias_Player != None
		player = Alias_Player.GetActorReference()
	EndIf
	If player != None && W05_Daily_Photo_DavenportMad != None
		player.SetValue(W05_Daily_Photo_DavenportMad, 1.0)
	EndIf
	SetStage(2000)
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetStage(2000)
EndFunction

Function Fragment_Stage_1300_Item_00()
	SetStage(2000)
EndFunction

Function Fragment_Stage_1400_Item_00()
	If Alias_Camera != None
		Alias_Camera.Clear()
	EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetStage(2000)
EndFunction

Function Fragment_Stage_2000_Item_00()
	Actor player = None
	If Alias_Player != None
		player = Alias_Player.GetActorReference()
	EndIf
	If player != None
		If W05_Daily_Photo_Completed != None
			player.SetValue(W05_Daily_Photo_Completed, 1.0)
		EndIf
		If W05_Daily_Photo_CompletedAtLeastOnce != None
			player.SetValue(W05_Daily_Photo_CompletedAtLeastOnce, 1.0)
		EndIf
	EndIf
	SetObjectiveCompleted(400, True)
	SetObjectiveCompleted(500, True)
	SetObjectiveCompleted(600, True)
	SetStage(1400)
	UnregisterForAllEvents()
	Stop()
EndFunction
