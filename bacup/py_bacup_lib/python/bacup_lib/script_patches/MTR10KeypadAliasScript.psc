MTR10_BattleQuestScript Function EventScript()
    Quest owner = GetOwningQuest()
    Return owner as MTR10_BattleQuestScript
EndFunction

Bool Function IsAlphaKeypad(MTR10_BattleQuestScript akEventScript)
    If akEventScript == None || MyKeypadGlobal == None
        Return True
    EndIf
    Return MyKeypadGlobal == akEventScript.MTR10_AlphaActivated
EndFunction

Form Function RequiredKeycard(MTR10_BattleQuestScript akEventScript, Bool abAlpha)
    ; FO76 read the card off KeyCardAlias, but that alias has no fill rule in the
    ; record, so fall back to the keycard the quest script binds for this panel.
    If KeyCardAlias != None && KeyCardAlias.GetReference() != None
        Return KeyCardAlias.GetReference().GetBaseObject()
    EndIf
    If akEventScript == None
        Return None
    EndIf
    If abAlpha
        Return akEventScript.MTR10KeycardAlpha
    EndIf
    Return akEventScript.MTR10KeycardBeta
EndFunction

Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = akActionRef as Actor
    If playerRef != Game.GetPlayer()
        Return
    EndIf
    MTR10_BattleQuestScript eventScript = EventScript()
    Bool isAlpha = IsAlphaKeypad(eventScript)
    Form keyForm = RequiredKeycard(eventScript, isAlpha)
    If keyForm != None && playerRef.GetItemCount(keyForm) > 0
        If KeypadsActiveGlobal != None
            KeypadsActiveGlobal.SetValue(1.0)
        EndIf
        If MyKeypadGlobal != None
            MyKeypadGlobal.SetValue(1.0)
        EndIf
        If SuccessMessage != None
            SuccessMessage.Show()
        EndIf
        ; The quest script owns the shared window: both panels have to be live at once.
        If eventScript != None
            eventScript.NotifyKeypadActivated(isAlpha)
        EndIf
    ElseIf FailMessage != None
        FailMessage.Show()
    EndIf
EndEvent
