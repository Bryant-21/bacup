Function Fragment_Stage_0001_Item_00()
	ObjectReference transponder = Alias_Transponder01.GetReference()
	Actor playerRef = Game.GetPlayer()
	If transponder != None && playerRef != None
		transponder.Activate(playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
	If pBoS03_Transponders_01 != None && !pBoS03_Transponders_01.IsPlaying()
		pBoS03_Transponders_01.Start()
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	If pBoS03_Transponders_01 != None && pBoS03_Transponders_01.IsPlaying()
		pBoS03_Transponders_01.Stop()
	EndIf
EndFunction
