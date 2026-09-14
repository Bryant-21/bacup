Function Fragment_Stage_0020_Item_00()
	If TW004PrematureKill != None
		TW004PrematureKill.Show()
	EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef != None
		playerRef.SetValue(TW004status, 2.0)
	EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
	If TW004PrematureKill != None
		TW004PrematureKill.Show()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	TW004Script hunt = TW004 as TW004Script
	If hunt != None
		hunt.CompleteHuntTarget(2)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef != None
		playerRef.SetValue(TW004status, 1.0)
	EndIf
	TW004.SetObjectiveDisplayed(100)
	If TW004a_Intro != None
		TW004a_Intro.Start()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	TW004Script hunt = TW004 as TW004Script
	If hunt != None
		hunt.CompleteHuntTarget(0)
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	If TW004PrematureKill != None
		TW004PrematureKill.Show()
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	TW004Script hunt = TW004 as TW004Script
	If hunt != None
		hunt.CompleteHuntTarget(1)
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
	TW004.SetObjectiveCompleted(100)
	TW004.SetObjectiveDisplayed(200)
	TW004.SetObjectiveDisplayed(300)
	TW004.SetObjectiveDisplayed(400)
EndFunction
