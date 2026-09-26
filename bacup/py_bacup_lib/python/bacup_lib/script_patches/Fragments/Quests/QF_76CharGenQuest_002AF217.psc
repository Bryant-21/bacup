; Stage fragments for 76CharGenQuest (002AF217), the Vault 76 character creation that the
; Tales FO76 alternate start sends through CharGenQuestKeyword (002AF218). The FO76
; client PEX has no fragment bodies. Order is recovered from the stage notes, the fragment
; and quest-script bindings and the surviving client bodies of CharGenFaceChairScript and
; CharGenPlayerActorScript (intro -> LooksMenu -> photo mode -> Pip-Boy console -> door).
; FO4 menus replace FO76's: ShowRaceMenu for the looks menu and ShowSPECIALMenu for the
; loadout. Photo mode (200 -> 210) is handled by the Tales alternate-start quest.

CharGenQuestScript Function CharGen()
	Return (Self as Quest) as CharGenQuestScript
EndFunction

Function Fragment_Stage_0000_Item_00()
	CharGen().BeginLocalCharGen()
	ObjectReference roomDoor = Alias_76CharGenRoomDoor.GetRef()
	If roomDoor
		roomDoor.SetOpen(False)
		roomDoor.BlockActivation(True, False)
	EndIf
	SetStage(100)
EndFunction

; Overseer mission mention; no bound effect survives.
Function Fragment_Stage_0050_Item_00()
EndFunction

; FO76 played its intro video here; the new game already played Fallout 4's.
Function Fragment_Stage_0100_Item_00()
	SetStage(150)
EndFunction

Function Fragment_Stage_0150_Item_00()
	Actor player = Game.GetPlayer()
	ObjectReference lights = Alias_76CharGenFaceLights.GetRef()
	If lights
		lights.Enable()
	EndIf
	player.SetHasCharGenSkeleton(True)
	player.ChangeAnimFaceArchetype(None)
	player.AddKeyword(CharGenPlayerInFaceGen)
	(Alias_currentPlayer as CharGenPlayerAliasScript).WatchFaceGen()
	Game.ShowRaceMenu()
EndFunction

Function Fragment_Stage_0200_Item_00()
	Actor player = Game.GetPlayer()
	player.RemoveKeyword(CharGenPlayerInFaceGen)
	player.ChangeAnimFaceArchetype(AnimFaceArchetypePlayer)
	ObjectReference lights = Alias_76CharGenFaceLights.GetRef()
	If lights
		lights.Disable()
	EndIf
	If !p76CharGenQuest_200_Start.IsPlaying()
		p76CharGenQuest_200_Start.Start()
	EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
	CharGen().TargetPipBoyObjective()
	SetObjectiveDisplayed(100)
	If !p76CharGenQuest_200_Start.IsPlaying() && !p76CharGenQuest_210_LoopingIntro.IsPlaying()
		p76CharGenQuest_210_LoopingIntro.Start()
	EndIf
	CharGen().WatchPipBoyConsole()
EndFunction

; Nuka-Tapper holotape taken (DefaultAliasInventoryManagement on the player alias).
Function Fragment_Stage_0215_Item_00()
EndFunction

Function Fragment_Stage_0220_Item_00()
	Game.GetPlayer().SetValue(CharGenHasPickedUpPipBoy, 1.0)
	CharGen().GivePipBoy()
	SetObjectiveCompleted(100)
	p76CharGenQuest_210_LoopingIntro.Stop()
EndFunction

Function Fragment_Stage_0250_Item_00()
	CharGen().ReleaseRoomDoor()
	ObjectReference roomDoor = Alias_76CharGenRoomDoor.GetRef()
	If roomDoor
		roomDoor.BlockActivation(False, False)
	EndIf
EndFunction

Function Fragment_Stage_0275_Item_00()
	If !CharGenQuest_275_BlockedRoomDoor.IsPlaying()
		CharGenQuest_275_BlockedRoomDoor.Start()
	EndIf
EndFunction

; SaveFailsafeTrigger: the player has left the room, so saving is allowed again.
Function Fragment_Stage_0280_Item_00()
	Game.SetInCharGen(False, False, False)
	Game.RequestAutoSave()
EndFunction

Function Fragment_Stage_0300_Item_00()
	If !p76CharGenQuest_300_AtriumBriefing.IsPlaying()
		p76CharGenQuest_300_AtriumBriefing.Start()
	EndIf
	; SURV_Tutorial is Reclamation Day; stage 13 shows "Discover the Overseer's mission".
	If SURV_Tutorial.IsRunning() && !SURV_Tutorial.GetStageDone(13)
		SURV_Tutorial.SetStage(13)
	EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
	Game.GetPlayer().SetValue(p76CharGenTalkedToCrutchley, 1.0)
EndFunction

; Stages 400-445 marked DEPRECATED in the source have no kiosk alias; nothing sets them.
Function Fragment_Stage_0400_Item_00()
EndFunction

Function Fragment_Stage_0405_Item_00()
EndFunction

Function Fragment_Stage_0410_Item_00()
	CharGen().GiveKioskItem(50, CharGen().LPI_76CharGen_Kiosk_Weapon_Melee_Hatchet)
EndFunction

Function Fragment_Stage_0415_Item_00()
	CharGen().GiveKioskItem(51, LL_76Chargen_Scrap_Start)
	ObjectReference bag = Alias_KioskItemBuildingSupplies.GetRef()
	If bag
		bag.BlockActivation(True, True)
		(bag as CharGenScrapBagActivatorScript).ClientDisableScrapBag()
	EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
EndFunction

Function Fragment_Stage_0425_Item_00()
EndFunction

Function Fragment_Stage_0430_Item_00()
EndFunction

; The C.A.M.P. kiosk has no deployable to hand over in Fallout 4.
Function Fragment_Stage_0435_Item_00()
	ObjectReference camp = Alias_KioskItemCAMP.GetRef()
	If camp
		camp.BlockActivation(True, True)
		(camp as CharGenCAMPActivatorScript).ClientDisableCAMP()
	EndIf
EndFunction

Function Fragment_Stage_0440_Item_00()
EndFunction

Function Fragment_Stage_0445_Item_00()
EndFunction

Function Fragment_Stage_0450_Item_00()
	CharGen().GiveKioskItem(58, CharGen().LPI_76CharGen_Kiosk_Weapon_Ranged_10mm_SemiAuto)
EndFunction

Function Fragment_Stage_0455_Item_00()
	CharGen().GiveKioskItem(59, CharGen().LPI_76CharGen_Kiosk_10mmAmmo)
EndFunction

; DEPRECATED perk board.
Function Fragment_Stage_0500_Item_00()
EndFunction

; Kiosk speaker triggers (600-660) record progress only: no speaker voice binding survives.
Function Fragment_Stage_0600_Item_00()
EndFunction

Function Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_0620_Item_00()
EndFunction

Function Fragment_Stage_0630_Item_00()
EndFunction

Function Fragment_Stage_0640_Item_00()
EndFunction

Function Fragment_Stage_0650_Item_00()
EndFunction

Function Fragment_Stage_0660_Item_00()
EndFunction

; LoadoutSelectTriggerScript already showed the SPECIAL menu.
Function Fragment_Stage_0800_Item_00()
EndFunction

Function Fragment_Stage_0900_Item_00()
	If !GetStageDone(800)
		Game.ShowSPECIALMenu()
	EndIf
	SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
	Game.GetPlayer().SetValue(CharGenComplete, 1.0)
	CharGen().FinishLocalCharGen()
EndFunction
