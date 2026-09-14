Actor Function ActorFromAlias(ReferenceAlias actorAlias)
	If actorAlias == None
		Return None
	EndIf
	Return actorAlias.GetActorReference()
EndFunction

Actor Function PlayerActor()
	Return ActorFromAlias(PlayerAlias)
EndFunction

Function EnableAlias(ReferenceAlias targetAlias)
	If targetAlias == None
		Return
	EndIf
	ObjectReference target = targetAlias.GetReference()
	If target != None
		target.Enable()
		Actor targetActor = target as Actor
		If targetActor != None
			targetActor.EvaluatePackage()
		EndIf
	EndIf
EndFunction

Function DisableAlias(ReferenceAlias targetAlias)
	If targetAlias != None && targetAlias.GetReference() != None
		targetAlias.GetReference().Disable()
	EndIf
EndFunction

Function EnableCollection(RefCollectionAlias targets)
	If targets == None
		Return
	EndIf
	Int index = 0
	While index < targets.GetCount()
		ObjectReference target = targets.GetAt(index)
		If target != None
			target.Enable()
			Actor targetActor = target as Actor
			If targetActor != None
				targetActor.EvaluatePackage()
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function DisableCollection(RefCollectionAlias targets)
	If targets == None
		Return
	EndIf
	Int index = 0
	While index < targets.GetCount()
		ObjectReference target = targets.GetAt(index)
		If target != None
			target.Disable()
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetActorEssential(ReferenceAlias actorAlias, Bool essential)
	Actor target = ActorFromAlias(actorAlias)
	If target != None && target.GetActorBase() != None
		target.GetActorBase().SetEssential(essential)
	EndIf
EndFunction

Function SetHostile(ReferenceAlias actorAlias, Faction hostileFaction)
	Actor target = ActorFromAlias(actorAlias)
	Actor player = PlayerActor()
	If target == None
		Return
	EndIf
	If MobsterFriendlyFaction != None
		target.RemoveFromFaction(MobsterFriendlyFaction)
	EndIf
	If hostileFaction != None
		target.AddToFaction(hostileFaction)
	EndIf
	If MobsterCombatantFaction != None
		target.AddToFaction(MobsterCombatantFaction)
	EndIf
	target.SetGhost(False)
	target.EvaluatePackage()
	If player != None
		target.StartCombat(player)
	EndIf
EndFunction

Function SetCollectionHostile(RefCollectionAlias actors, Faction hostileFaction)
	If actors == None
		Return
	EndIf
	Int index = 0
	While index < actors.GetCount()
		Actor target = actors.GetAt(index) as Actor
		If target != None
			If MobsterFriendlyFaction != None
				target.RemoveFromFaction(MobsterFriendlyFaction)
			EndIf
			If hostileFaction != None
				target.AddToFaction(hostileFaction)
			EndIf
			target.SetGhost(False)
			target.EvaluatePackage()
		EndIf
		index += 1
	EndWhile
EndFunction

Function GiveItemIfMissing(Form itemToGive)
	Actor player = PlayerActor()
	If player != None && itemToGive != None && player.GetItemCount(itemToGive) < 1
		player.AddItem(itemToGive, 1, True)
	EndIf
EndFunction

Function RemovePlayerItem(Form itemToRemove)
	Actor player = PlayerActor()
	If player != None && itemToRemove != None
		Int itemCount = player.GetItemCount(itemToRemove)
		If itemCount > 0
			player.RemoveItem(itemToRemove, itemCount, True)
		EndIf
	EndIf
EndFunction

Function SetPlayerValue(ActorValue valueToSet, Float value)
	Actor player = PlayerActor()
	If player != None && valueToSet != None
		player.SetValue(valueToSet, value)
	EndIf
EndFunction

Function PrepareConcerta()
	Actor concertaRef = ActorFromAlias(Concerta)
	ObjectReference marker = ConcertaTeleportMarker.GetReference()
	If concertaRef != None
		concertaRef.Enable()
		If marker != None
			concertaRef.MoveTo(marker)
		EndIf
		concertaRef.EvaluatePackage()
	EndIf
	EnableCollection(ConcertaSecurityDetail)
EndFunction

Function Fragment_Stage_0100_Item_00()
	EnableAlias(Flyer)
EndFunction

Function Fragment_Stage_0105_Item_00()
	EnableAlias(Flyer)
EndFunction

Function Fragment_Stage_0110_Item_00()
	SetObjectiveDisplayed(12)
EndFunction

Function Fragment_Stage_0115_Item_00()
	EnableAlias(Alias_Door_BoardwalkToCasinoQuarter)
	EnableAlias(Fabio)
EndFunction

Function Fragment_Stage_0116_Item_00()
	SetObjectiveCompleted(12)
	SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0117_Item_00()
	SetActorEssential(Fabio, False)
EndFunction

Function Fragment_Stage_0119_Item_00()
	EnableAlias(Fabio)
	EnableCollection(FabioPosse)
EndFunction

Function Fragment_Stage_0125_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveDisplayed(20)
	EnableAlias(Jordy)
	EnableCollection(GangMembers)
EndFunction

Function Fragment_Stage_0127_Item_00()
	EnableAlias(Jordy)
	EnableCollection(GangMembers)
EndFunction

Function Fragment_Stage_0129_Item_00()
	SetHostile(Jordy, GangEnemyFaction)
	SetCollectionHostile(GangMembers, GangEnemyFaction)
EndFunction

Function Fragment_Stage_0130_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0135_Item_00()
	SetObjectiveCompleted(25)
	SetObjectiveDisplayed(30)
	EnableAlias(Fabio)
EndFunction

Function Fragment_Stage_0140_Item_00()
	EnableAlias(Alias_Door_BoardwalkToCasinoQuarter)
	DisableCollection(OvergrownDisabler)
EndFunction

Function Fragment_Stage_0141_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0145_Item_00()
	SetObjectiveCompleted(35)
	SetObjectiveDisplayed(40)
	SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0152_Item_00()
	SetObjectiveCompleted(45)
EndFunction

Function Fragment_Stage_0155_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveCompleted(45)
	SetObjectiveDisplayed(50)
	SetObjectiveDisplayed(52)
EndFunction

Function Fragment_Stage_0161_Item_00()
	GiveItemIfMissing(BookKey)
EndFunction

Function Fragment_Stage_0162_Item_00()
	GiveItemIfMissing(DevilsBlood)
EndFunction

Function Fragment_Stage_0165_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveCompleted(52)
	SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0170_Item_00()
	SetObjectiveCompleted(55)
	GiveItemIfMissing(PianoNote)
	SetObjectiveDisplayed(57)
EndFunction

Function Fragment_Stage_0171_Item_00()
EndFunction

Function Fragment_Stage_0172_Item_00()
	SetObjectiveCompleted(57)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0175_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0180_Item_00()
	SetObjectiveCompleted(65)
	SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0185_Item_00()
	SetPlayerValue(Ending, 0.0)
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(71)
	ObjectReference hiddenDoor = BookcaseDoor.GetReference()
	If hiddenDoor != None
		hiddenDoor.Lock(False)
	EndIf
EndFunction

Function Fragment_Stage_0252_Item_00()
	SetObjectiveCompleted(71)
	SetObjectiveDisplayed(75)
	EnableAlias(Quentino)
EndFunction

Function Fragment_Stage_0255_Item_00()
	PrepareConcerta()
EndFunction

Function Fragment_Stage_0260_Item_00()
	SetObjectiveCompleted(75)
	SetObjectiveDisplayed(80)
	PrepareConcerta()
EndFunction

Function Fragment_Stage_0269_Item_00()
	If !IsStageDone(270)
		SetStage(270)
	EndIf
EndFunction

Function Fragment_Stage_0270_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(85)
	PrepareConcerta()
EndFunction

Function Fragment_Stage_0272_Item_00()
	SetPlayerValue(Ending, 1.0)
	SetObjectiveCompleted(85)
	SetObjectiveDisplayed(90)
	SetObjectiveDisplayed(95)
EndFunction

Function Fragment_Stage_0274_Item_00()
	SetPlayerValue(Ending, 2.0)
	SetObjectiveCompleted(85)
	SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0276_Item_00()
	SetPlayerValue(Ending, 3.0)
	SetObjectiveCompleted(85)
	SetObjectiveDisplayed(100)
	SetActorEssential(Fabio, False)
EndFunction

Function Fragment_Stage_0277_Item_00()
	SetPlayerValue(IsConcertaDead, 1.0)
	SetObjectiveCompleted(85)
	SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0278_Item_00()
	SetHostile(Concerta, ConcertaHostileFaction)
	SetCollectionHostile(ConcertaSecurityDetail, ConcertaHostileFaction)
EndFunction

Function Fragment_Stage_0279_Item_00()
	SetActorEssential(Fabio, False)
EndFunction

Function Fragment_Stage_0280_Item_00()
	SetPlayerValue(Ending, 4.0)
	SetObjectiveCompleted(85)
	SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0281_Item_00()
	SetObjectiveDisplayed(100)
	SetHostile(Fabio, GangEnemyFaction)
	SetCollectionHostile(FabioPosse, GangEnemyFaction)
EndFunction

Function Fragment_Stage_0282_Item_00()
	SetObjectiveCompleted(85)
	SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0284_Item_00()
	SetPlayerValue(IsFabioDead_AV, 1.0)
	SetObjectiveCompleted(100)
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0286_Item_00()
	SetPlayerValue(Ending, 5.0)
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0288_Item_00()
	SetPlayerValue(Ending, 6.0)
	SetObjectiveCompleted(90)
	SetObjectiveCompleted(95)
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0290_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveCompleted(95)
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(110)
	RemovePlayerItem(BookKey)
	RemovePlayerItem(DevilsBlood)
	RemovePlayerItem(PianoNote)
	DisableCollection(FabioPosse)
	DisableCollection(ConcertaSecurityDetail)
EndFunction
