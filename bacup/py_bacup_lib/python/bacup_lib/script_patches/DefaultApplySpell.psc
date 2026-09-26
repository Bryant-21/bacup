Event OnQuestInit()
    currentStageIndex = -1
    bTimerActive = False
    CurrentSpellsToApply = new SpellsAndActors[0]
    currentSoundsToPlay = new SoundProperties[0]
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Int index = 0
    While StagesProperties != None && index < StagesProperties.Length
        If StagesProperties[index] != None && StagesProperties[index].iPreReqStage == auiStageID
            ApplyStage(index)
        EndIf
        index += 1
    EndWhile
EndEvent

; Timer ids 7660+ replay a delayed sound row; 7800+ set a stage row's delayed next stage.
Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf
    If aiTimerID >= 7660 && aiTimerID < 7660 + 128
        PlaySoundRow(aiTimerID - 7660)
    ElseIf aiTimerID >= 7800 && aiTimerID < 7800 + 128
        bTimerActive = False
        StagesStruct stage = StagesProperties[aiTimerID - 7800]
        If stage != None && stage.iStageToSet >= 0 && !IsStageDone(stage.iStageToSet)
            SetStage(stage.iStageToSet)
        EndIf
    EndIf
EndEvent

Function ApplyStage(Int aiStageIndex)
    currentStageIndex = aiStageIndex
    CurrentSpellsToApply = new SpellsAndActors[0]
    currentSoundsToPlay = new SoundProperties[0]
    Int index = 0
    While SpellsToApply != None && index < SpellsToApply.Length
        SpellsAndActors row = SpellsToApply[index]
        If row != None && row.iStageIndex == aiStageIndex && row.SpellToCast != None
            CurrentSpellsToApply.Add(row)
            CastOnAlias(row.SpellToCast, row.ActorAlias)
            CastOnAlias(row.SpellToCast, row.PlayerAlias)
            CastOnCollection(row.SpellToCast, row.EWSRefCollection)
            CastOnCollection(row.SpellToCast, row.PlayersRefCollection)
        EndIf
        index += 1
    EndWhile
    index = 0
    While SoundsToPlay != None && index < SoundsToPlay.Length && index < 128
        SoundProperties soundRow = SoundsToPlay[index]
        If soundRow != None && soundRow.iStageIndex == aiStageIndex && soundRow.SoundToPlay != None
            currentSoundsToPlay.Add(soundRow)
            If soundRow.fPlayAfter > 0.0
                StartTimer(soundRow.fPlayAfter, 7660 + index)
            Else
                PlaySoundRow(index)
            EndIf
        EndIf
        index += 1
    EndWhile
    StagesStruct stage = StagesProperties[aiStageIndex]
    If stage.iStageToSet >= 0 && !IsStageDone(stage.iStageToSet)
        If stage.fNextStageDelay > 0.0 && aiStageIndex < 128
            bTimerActive = True
            StartTimer(stage.fNextStageDelay, 7800 + aiStageIndex)
        Else
            SetStage(stage.iStageToSet)
        EndIf
    EndIf
EndFunction

Function CastOnAlias(Spell akSpell, ReferenceAlias akAlias)
    If akAlias == None
        Return
    EndIf
    Actor target = akAlias.GetActorReference()
    If target != None && !target.IsDead()
        akSpell.Cast(target, target)
    EndIf
EndFunction

Function CastOnCollection(Spell akSpell, RefCollectionAlias akCollection)
    If akCollection == None
        Return
    EndIf
    Int index = akCollection.GetCount() - 1
    While index >= 0
        Actor target = akCollection.GetAt(index) as Actor
        If target != None && !target.IsDead()
            akSpell.Cast(target, target)
        EndIf
        index -= 1
    EndWhile
EndFunction

Function PlaySoundRow(Int aiSoundIndex)
    SoundProperties row = SoundsToPlay[aiSoundIndex]
    If row == None || row.SoundToPlay == None
        Return
    EndIf
    If row.SoundSource != None && row.SoundSource.GetReference() != None
        row.SoundToPlay.Play(row.SoundSource.GetReference())
    EndIf
    If row.SoundSources != None
        Int index = row.SoundSources.GetCount() - 1
        While index >= 0
            ObjectReference source = row.SoundSources.GetAt(index)
            If source != None
                row.SoundToPlay.Play(source)
            EndIf
            index -= 1
        EndWhile
    EndIf
EndFunction
