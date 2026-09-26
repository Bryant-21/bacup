Event OnAliasInit()
	OwningQuest = GetOwningQuest() as E09D_GWWS_QuestScript
	ClearSmoke()
EndEvent

Event OnAliasShutdown()
	ClearSmoke()
EndEvent

E09D_GWWS_QuestScript Function EventScript()
	If OwningQuest == None
		OwningQuest = GetOwningQuest() as E09D_GWWS_QuestScript
	EndIf
	Return OwningQuest
EndFunction

Event OnActivate(ObjectReference akActionRef)
	Actor player = Game.GetPlayer()
	E09D_GWWS_QuestScript eventScript = EventScript()
	If akActionRef != player || eventScript == None || !eventScript.IsScoring()
		Return
	EndIf
	Int deposited = 0
	Int index = 0
	While ItemsToRemove != None && index < ItemsToRemove.Length
		ItemToRemove row = ItemsToRemove[index]
		If row != None && row.Item != None
			Int count = player.GetItemCount(row.Item)
			If count > 0
				player.RemoveItem(row.Item, count, False)
				deposited += count * row.Score
			EndIf
		EndIf
		index += 1
	EndWhile
	deposited += DepositCutout(player, CappyItem, CappyTopic, CappyDepositStage)
	deposited += DepositCutout(player, BottleItem, BottleTopic, BottleDepositStage)
	Quest owner = GetOwningQuest()
	If owner.IsStageDone(CappyDepositStage) && owner.IsStageDone(BottleDepositStage) && !owner.IsStageDone(BothDepositStage)
		owner.SetStage(BothDepositStage)
	EndIf
	If deposited > 0
		eventScript.AddScore(deposited)
	EndIf
EndEvent

Int Function DepositCutout(Actor akPlayer, Form akCutout, Topic akTopic, Int aiDepositStage)
	If akCutout == None || akPlayer.GetItemCount(akCutout) <= 0
		Return 0
	EndIf
	Int count = akPlayer.GetItemCount(akCutout)
	akPlayer.RemoveItem(akCutout, count, False)
	If akTopic != None
		akPlayer.Say(akTopic, None, True)
	EndIf
	Quest owner = GetOwningQuest()
	If aiDepositStage >= 0 && !owner.IsStageDone(aiDepositStage)
		owner.SetStage(aiDepositStage)
	EndIf
	Return count * CutoutScore
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	Quest owner = GetOwningQuest()
	; ClearDestruction on reset lowers the stage; only fresh damage in a live round announces anything.
	If aiCurrentStage <= aiOldStage || owner == None || !owner.IsRunning()
		Return
	EndIf
	AnnounceDamage(owner, wagonHealth75, aiCurrentStage)
	AnnounceDamage(owner, wagonHealth50, aiCurrentStage)
	AnnounceDamage(owner, wagonHealth25, aiCurrentStage)
	If wagonHealth25 != None && aiCurrentStage >= wagonHealth25.wagonDamageStage
		PlaceSmoke()
	EndIf
EndEvent

Function AnnounceDamage(Quest akQuest, DamageVoiceStruct akVoice, Int aiDestructionStage)
	If akVoice == None || akVoice.QuestStageToSet < 0 || aiDestructionStage < akVoice.wagonDamageStage
		Return
	EndIf
	If !akQuest.IsStageDone(akVoice.QuestStageToSet)
		akQuest.SetStage(akVoice.QuestStageToSet)
	EndIf
EndFunction

Function PlaceSmoke()
	ObjectReference wagonRef = GetReference()
	If wagonRef == None || SmokeEffectForm == None || SmokeEffectRef != None
		Return
	EndIf
	SmokeEffectRef = wagonRef.PlaceAtMe(SmokeEffectForm, 1, False, False, True)
EndFunction

Function ClearSmoke()
	If SmokeEffectRef == None
		Return
	EndIf
	SmokeEffectRef.Disable(False)
	SmokeEffectRef.Delete()
	SmokeEffectRef = None
EndFunction
