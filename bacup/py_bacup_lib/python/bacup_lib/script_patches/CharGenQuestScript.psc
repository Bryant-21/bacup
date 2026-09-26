; Single-player replacement for the server half of FO76 character creation. Only the
; Tales FO76 alternate start sends CharGenQuestKeyword, so none of this runs after MQ101.

Function BeginLocalCharGen()
	Actor player = Game.GetPlayer()
	Game.SetInCharGen(True, True, False)
	Game.SetCharGenHUDMode(1)
	If !bGavePlayerCharGenItems
		bGavePlayerCharGenItems = True
		player.AddItem(Armor_VaultSuit76_Underarmor_Clean, 1, True)
		player.EquipItem(Armor_VaultSuit76_Underarmor_Clean, False, True)
	EndIf
	ObjectReference collision = Alias_76CharGenRoomDoorCollision.GetRef()
	If collision
		collision.Enable()
	EndIf
EndFunction

; The FO76 face chair watched the Pip-Boy console for the player's activation.
Function WatchPipBoyConsole()
	ObjectReference console = Alias_76CharGenPickUpPipBoy.GetRef()
	If console
		console.Enable()
		RegisterForRemoteEvent(console, "OnActivate")
	EndIf
EndFunction

; 76CharGenPipboyObjective (alias 66) is optional and fills from a marker linked to the
; start marker; when that fill is empty the objective shows without a compass marker.
Function TargetPipBoyObjective()
	ReferenceAlias objective = GetAlias(66) as ReferenceAlias
	ObjectReference console = Alias_76CharGenPickUpPipBoy.GetRef()
	If objective && console && !objective.GetRef()
		objective.ForceRefTo(console)
	EndIf
	; FO4 draws compass markers only for the tracked quest; Reclamation Day starts first.
	SetActive(True)
	Debug.Trace("76CharGenQuest: Pip-Boy objective target=" + objective.GetRef() + " console=" + console)
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && akSender == Alias_76CharGenPickUpPipBoy.GetRef()
		UnregisterForRemoteEvent(akSender, "OnActivate")
		If !GetStageDone(220)
			SetStage(220)
		EndIf
	EndIf
EndEvent

Function GivePipBoy()
	Actor player = Game.GetPlayer()
	If player.GetItemCount(Pipboy) == 0
		player.AddItem(Pipboy, 1, True)
	EndIf
	player.EquipItem(Pipboy, False, True)
	Game.SetCharGenHUDMode(0)
	; The console seat normally ends with OnGetUp; this covers a console that cannot seat.
	StartTimer(5.0, AcquiredPipBoyStage)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == AcquiredPipBoyStage && GetStageDone(220) && !GetStageDone(AcquiredPipBoyStage)
		SetStage(AcquiredPipBoyStage)
	EndIf
EndEvent

Function ReleaseRoomDoor()
	CancelTimer(AcquiredPipBoyStage)
	ObjectReference collision = Alias_76CharGenRoomDoorCollision.GetRef()
	If collision
		collision.Disable()
	EndIf
	; The collision also links to the start marker; cover an unfilled optional alias.
	ObjectReference startMarker = (GetAlias(2) as ReferenceAlias).GetRef()
	Keyword collisionLink = Game.GetFormFromFile(0x3A389B, "SeventySix.esm") as Keyword
	Int linkedCount = 0
	If startMarker && collisionLink
		ObjectReference[] linked = startMarker.GetLinkedRefChildren(collisionLink)
		linkedCount = linked.Length
		Int i = 0
		While i < linked.Length
			linked[i].Disable()
			i += 1
		EndWhile
	EndIf
	Debug.Trace("76CharGenQuest: room door released; collision alias=" + collision + " linked collisions=" + linkedCount)
EndFunction

; Kiosk items are placed display dummies; taking one swaps it for the bound supply.
Function GiveKioskItem(Int aiKioskAlias, Form akSupply)
	Actor player = Game.GetPlayer()
	ReferenceAlias kiosk = GetAlias(aiKioskAlias) as ReferenceAlias
	ObjectReference display
	If kiosk
		display = kiosk.GetRef()
	EndIf
	If display && player.GetItemCount(display) > 0
		player.RemoveItem(display, 1, True)
	ElseIf display && !(display.GetBaseObject() as Activator)
		display.Disable()
	EndIf
	If akSupply
		player.AddItem(akSupply, 1, False)
	EndIf
EndFunction

Function FinishLocalCharGen()
	Actor player = Game.GetPlayer()
	CancelTimer(AcquiredPipBoyStage)
	Game.SetInCharGen(False, False, False)
	Game.SetCharGenHUDMode(0)
	If VaultFedSpell
		VaultFedSpell.Cast(player, player)
	EndIf
EndFunction
