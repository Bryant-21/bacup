Event OnActivate(ObjectReference akActionRef)
    Actor enteringActor = akActionRef as Actor
    If enteringActor == None || pSpell == None
        Return
    EndIf

    enteringActor.AddSpell(pSpell)
    CurrentUser = enteringActor
EndEvent

; OnExitFurniture fires separately from OnActivate for furniture (FO4 wiki), so the
; entering actor is tracked in CurrentUser and re-checked here rather than trusted
; from akActionRef alone.
Event OnExitFurniture(ObjectReference akActionRef)
    If bRemoveSpellOnExit && CurrentUser != None && (akActionRef as Actor) == CurrentUser
        CurrentUser.RemoveSpell(pSpell)
        CurrentUser = None
    EndIf
EndEvent
