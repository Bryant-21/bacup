ObjectReference Function PlayerReference()
	Return Alias_Player.GetReference()
EndFunction

Function GiveIfMissing(Form itemToGive)
	ObjectReference player = PlayerReference()
	If itemToGive && player.GetItemCount(itemToGive) == 0
		player.AddItem(itemToGive, 1, True)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(5)
	PlayerReference().SetValue(Storm_MQ_AudreyAwayValue, 1.0)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(5)
	SetObjectiveDisplayed(10)
	GiveIfMissing(VisitorCenterKey)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0310_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveCompleted(35)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0505_Item_00()
	SetObjectiveCompleted(45)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0507_Item_00()
	If !KevinArrivedAtBunkerScene.IsPlaying()
		KevinArrivedAtBunkerScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0508_Item_00()
	Actor visitorKevin = Alias_Actor_Kevin.GetActorReference()
	Actor bunkerKevin = Alias_Actor_Kevin_Bunker.GetActorReference()
	ObjectReference bunkerMarker = Alias_Marker_BunkerIntKevinTeleport.GetReference()
	If visitorKevin
		visitorKevin.Disable()
	EndIf
	If bunkerKevin
		bunkerKevin.Enable()
		If bunkerMarker
			bunkerKevin.MoveTo(bunkerMarker)
		EndIf
		bunkerKevin.EvaluatePackage()
	EndIf
	If !IsStageDone(509)
		SetStage(509)
	EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0610_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(80)
	Alias_Static_ElectricSparks.GetReference().Disable()
	Alias_BrokenPanel.GetReference().Disable()
	Alias_RepairedPanel.GetReference().Enable()
	Storm_MQ06_HugoPt3_StartKeyword.SendStoryEvent(None, PlayerReference(), PlayerReference())
	SetStage(9999)
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction

; Stages 50/60/70 are FO76 private-instance provisioning ("Visitor Center / Bunker /
; WeatherLab Instance Set up"). Fallout 4 has no instancing, so the spaces these stages
; would spin up are ordinary always-present cells and the member is an intentional no-op.
Function Fragment_Stage_0050_Item_00()
EndFunction

Function Fragment_Stage_0060_Item_00()
EndFunction

Function Fragment_Stage_0070_Item_00()
EndFunction

Function Fragment_Stage_0520_Item_00()
	If !IsObjectiveCompleted(60)
		SetObjectiveDisplayed(60)
	EndIf
EndFunction
