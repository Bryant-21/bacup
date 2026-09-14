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
