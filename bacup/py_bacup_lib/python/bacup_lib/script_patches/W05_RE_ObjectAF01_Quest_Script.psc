Event OnQuestInit()
    ObjectReference protectronRef = DisProtectron.GetReference()
    If protectronRef != None
        RegisterForRemoteEvent(protectronRef, "OnActivate")
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    ObjectReference protectronRef = DisProtectron.GetReference()
    If akSender == protectronRef && akActionRef == Game.GetPlayer() && !IsStageDone(20) && W05_RE_ObjectAF01_Protectron_InteractMSG != None
        W05_RE_ObjectAF01_Protectron_InteractMSG.Show()
    EndIf
EndEvent
