Function Fragment_Stage_0010_Item_00()
	SetStage(50)
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(100, True)
	SetStage(175)

	Int activity = SFS02_Play_ActivityGlobal.GetValueInt()
	If activity < 1 || activity > 4
		activity = Utility.RandomInt(1, 4)
		SFS02_Play_ActivityGlobal.SetValueInt(activity)
	EndIf

	If activity == 1
		SetStage(200)
	ElseIf activity == 2
		SetStage(300)
	ElseIf activity == 3
		SetStage(400)
	Else
		SetStage(500)
	EndIf
EndFunction

Function Fragment_Stage_0175_Item_00()
	Actor playerRef = Alias_SFS02Player.GetActorReference()
	If playerRef != None && playerRef.GetValue(SFS02_Play_QuestTrackingValue) > 0.0
		SFS02_Play_SophieIntro.Start()
	Else
		SFS02_Play_ChloeIntro.Start()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveDisplayed(200, True)
	SetObjectiveDisplayed(210, True)
	SetObjectiveDisplayed(220, True)
EndFunction

Function Fragment_Stage_0210_Item_00()
	SetObjectiveCompleted(200, True)
	If IsStageDone(211) && IsStageDone(212)
		SetStage(250)
	EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
	SetObjectiveCompleted(210, True)
	If IsStageDone(210) && IsStageDone(212)
		SetStage(250)
	EndIf
EndFunction

Function Fragment_Stage_0212_Item_00()
	SetObjectiveCompleted(220, True)
	If IsStageDone(210) && IsStageDone(211)
		SetStage(250)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	Actor playerRef = Alias_SFS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(SFS02_Play_FrogTrackingValue, 1.0)
	EndIf
	SetStage(600)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetObjectiveCompleted(300, True)
	Actor playerRef = Alias_SFS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(SFS02_Play_FlowerTrackingValue, 1.0)
	EndIf
	SetStage(600)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
	SetObjectiveCompleted(400, True)
	SetObjectiveDisplayed(450, True)
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(450, True)
	Actor playerRef = Alias_SFS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(SFS02_Play_PlayDateTrackingValue, 1.0)
	EndIf
	SetStage(600)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0510_Item_00()
	If IsStageDone(511) && IsStageDone(512)
		SetStage(550)
	EndIf
EndFunction

Function Fragment_Stage_0511_Item_00()
	If IsStageDone(510) && IsStageDone(512)
		SetStage(550)
	EndIf
EndFunction

Function Fragment_Stage_0512_Item_00()
	If IsStageDone(510) && IsStageDone(511)
		SetStage(550)
	EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
	SetObjectiveCompleted(500, True)
	Actor playerRef = Alias_SFS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(SFS02_Play_ToyTrackingValue, 1.0)
	EndIf
	SetStage(600)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveDisplayed(600, True)
	SFS02_Play_ChloeEnd.Start()
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(600, True)
	Actor playerRef = Alias_SFS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(SFS02_Play_QuestTrackingValue, 1.0)
	EndIf
	SFS02_Play_ActivityGlobal.SetValueInt(0)
	CompleteQuest()
	Stop()
EndFunction
