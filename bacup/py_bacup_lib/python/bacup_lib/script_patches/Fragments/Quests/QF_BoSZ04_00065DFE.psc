Function Fragment_Stage_0001_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 1 fragment running quest=" + Self as String, 0)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && pBoSz04StartedAV != None
		playerRef.SetValue(pBoSz04StartedAV, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 2 fragment running quest=" + Self as String, 0)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef != None && pLvlVulture != None
		playerRef.PlaceAtMe(pLvlVulture, 1, False, False, True)
	EndIf
EndFunction

Function Fragment_Stage_0025_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 25 fragment running", 0)
	SetObjectiveDisplayed(25, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 50 fragment running", 0)
	SetObjectiveCompleted(25, True)
	SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0075_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 75 fragment running", 0)
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(75, True)
EndFunction

Function Fragment_Stage_0080_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 80 fragment running", 0)
	SetObjectiveCompleted(75, True)
	SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_0095_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 95 fragment running", 0)
	SetObjectiveCompleted(80, True)
	SetObjectiveDisplayed(95, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 100 fragment running", 0)
	If IsObjectiveDisplayed(95)
		SetObjectiveCompleted(95, True)
	EndIf
	SetObjectiveDisplayed(100, True)

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && pBoSZ04HarvestSBDNAPerk != None && !playerRef.HasPerk(pBoSZ04HarvestSBDNAPerk)
		playerRef.AddPerk(pBoSZ04HarvestSBDNAPerk)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 200 fragment running", 0)
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)

	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If pBoSZ04HarvestSBDNAPerk != None && playerRef.HasPerk(pBoSZ04HarvestSBDNAPerk)
			playerRef.RemovePerk(pBoSZ04HarvestSBDNAPerk)
		EndIf
		If pBoSZ04VultureDNA != None && playerRef.GetItemCount(pBoSZ04VultureDNA) == 0
			playerRef.AddItem(pBoSZ04VultureDNA, 1, False)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 300 fragment running", 0)
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && pBoSZ04VultureDNA != None && playerRef.GetItemCount(pBoSZ04VultureDNA) > 0
		playerRef.RemoveItem(pBoSZ04VultureDNA, 1, True)
	EndIf
	If pBoSZ04VultureDNALoadedMessage != None
		pBoSZ04VultureDNALoadedMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 350 fragment running", 0)
	SetObjectiveCompleted(300, True)
	SetStage(500)
EndFunction

Function Fragment_Stage_0500_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 500 fragment stopping quest", 0)
	Stop()
EndFunction

Function Fragment_Stage_9900_Item_00()
	Debug.Trace("[B21 BoSZ04] Quest stage 9900 cleanup fragment running", 0)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If pBoSZ04HarvestSBDNAPerk != None && playerRef.HasPerk(pBoSZ04HarvestSBDNAPerk)
			playerRef.RemovePerk(pBoSZ04HarvestSBDNAPerk)
		EndIf
		If pBoSZ04VultureDNA != None && playerRef.GetItemCount(pBoSZ04VultureDNA) > 0
			playerRef.RemoveItem(pBoSZ04VultureDNA, playerRef.GetItemCount(pBoSZ04VultureDNA), True)
		EndIf
	EndIf
EndFunction
