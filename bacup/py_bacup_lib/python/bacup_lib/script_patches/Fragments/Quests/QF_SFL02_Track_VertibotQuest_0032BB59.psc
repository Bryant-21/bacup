Function Fragment_Stage_0100_Item_00()
	If SFL02_Track != None && SFL02_Track.GetCurrentStageID() < 500
		SFL02_Track.SetStage(500)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	If SFL02_Track != None && SFL02_Track.GetCurrentStageID() < 550
		SFL02_Track.SetStage(550)
	EndIf
	If SFL02_Track_SignalBoosterSFXEnableRef != None
		SFL02_Track_SignalBoosterSFXEnableRef.Enable(False)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	If SFL02_Track != None && SFL02_Track.GetCurrentStageID() < 800
		SFL02_Track.SetStage(800)
	EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
	Stop()
EndFunction
