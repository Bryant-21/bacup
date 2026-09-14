Bool Function LowerAtriumTasksComplete()
	Return IsStageDone(410) && IsStageDone(420) && IsStageDone(430)
EndFunction

Function AdvanceWhenLowerAtriumTasksComplete()
	If LowerAtriumTasksComplete() && !IsStageDone(499)
		SetStage(499)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(31)
	SetObjectiveDisplayed(32)
	SetObjectiveDisplayed(33)
EndFunction

Function Fragment_Stage_0410_Item_00()
	SetObjectiveCompleted(31)
EndFunction

Function Fragment_Stage_0410_Item_01()
	AdvanceWhenLowerAtriumTasksComplete()
EndFunction

Function Fragment_Stage_0417_Item_00()
	SetObjectiveCompleted(35)
EndFunction

Function Fragment_Stage_0417_Item_01()
	AdvanceWhenLowerAtriumTasksComplete()
EndFunction

Function Fragment_Stage_0420_Item_00()
	SetObjectiveCompleted(32)
	SetObjectiveCompleted(34)
EndFunction

Function Fragment_Stage_0420_Item_01()
	AdvanceWhenLowerAtriumTasksComplete()
EndFunction

Function Fragment_Stage_0425_Item_00()
	SetObjectiveDisplayed(36)
EndFunction

Function Fragment_Stage_0427_Item_00()
	SetObjectiveCompleted(36)
EndFunction

Function Fragment_Stage_0430_Item_00()
	SetObjectiveCompleted(33)
EndFunction

Function Fragment_Stage_0430_Item_01()
	AdvanceWhenLowerAtriumTasksComplete()
EndFunction

Function Fragment_Stage_0480_Item_00()
	AdvanceWhenLowerAtriumTasksComplete()
EndFunction

Function Fragment_Stage_0499_Item_00()
	SetObjectiveCompleted(31)
	SetObjectiveCompleted(32)
	SetObjectiveCompleted(33)
	SetObjectiveCompleted(34)
	SetObjectiveCompleted(35)
	SetObjectiveCompleted(36)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
	SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0550_Item_00()
	SetObjectiveCompleted(55)
	Alias_Player.GetReference().SetValue(GossipAV, 1.0)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
	SetObjectiveDisplayed(61)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveCompleted(61)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	Storm_MQ08_OberlinPt2.SendStoryEvent()
EndFunction

Function Fragment_Stage_9999_Item_00()
	Alias_RefCol_RepairablePipes.RemoveAll()
	Alias_RefCol_FixedPipes.RemoveAll()
EndFunction

Function Fragment_Stage_0415_Item_00()
	SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0510_Item_00()
	If !IsObjectiveCompleted(50)
		SetObjectiveDisplayed(50)
	EndIf
	If !IsObjectiveCompleted(55)
		SetObjectiveDisplayed(55)
	EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
	If !IsObjectiveCompleted(60)
		SetObjectiveDisplayed(60)
	EndIf
	If !IsObjectiveCompleted(61)
		SetObjectiveDisplayed(61)
	EndIf
EndFunction
