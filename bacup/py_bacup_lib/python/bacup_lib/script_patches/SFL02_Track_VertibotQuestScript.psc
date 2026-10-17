; SFL02 signal-booster vertibot: the attach perk (SFL02_Track_PerkScript) sets 200 when the player fits the booster;
; the install is heard at the vertibot. The scene and the patched stage fragments drive the rest.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != 200 || QSTSFL02SignalBoostInstall == None
        Return
    EndIf
    ObjectReference source = Vertibot.GetReference()
    If source == None
        source = Game.GetPlayer()
    EndIf
    QSTSFL02SignalBoostInstall.Play(source)
EndEvent
