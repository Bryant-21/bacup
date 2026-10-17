; RE_SceneKMK06 vertibird flyover. Vertibird01 always appears; the initially disabled Vertibird02/03 each join with
; their enableChance (0.25 / 0.10). Each one's scene reports SceneDoneStage on its own.
Event OnQuestInit()
    If Vertibirds == None
        Return
    EndIf
    Int index = 0
    While index < Vertibirds.Length
        vertibirdData entry = Vertibirds[index]
        If entry != None && entry.Alias_Vertibird != None
            ObjectReference vertibird = entry.Alias_Vertibird.GetReference()
            If vertibird != None && vertibird.IsDisabled() && Utility.RandomFloat(0.0, 1.0) <= entry.enableChance
                vertibird.Enable()
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent
