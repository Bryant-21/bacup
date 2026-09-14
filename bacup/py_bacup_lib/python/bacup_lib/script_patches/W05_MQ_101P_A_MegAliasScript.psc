Event OnAliasInit()
    PushTopOfTheWorldRefs()
EndEvent

Function PushTopOfTheWorldRefs()
    W05_MQ_101P_QuestScript controller = W05_MQ_101P as W05_MQ_101P_QuestScript
    If !controller
        Return
    EndIf
    ObjectReference megRef = None
    ObjectReference raiderARef = None
    ObjectReference raiderBRef = None
    If MegAtToTW
        megRef = MegAtToTW.GetReference()
    EndIf
    If RaiderAAtToTW
        raiderARef = RaiderAAtToTW.GetReference()
    EndIf
    If RaiderBAtToTW
        raiderBRef = RaiderBAtToTW.GetReference()
    EndIf
    controller.FillTopOfTheWorldAliases(megRef, raiderARef, raiderBRef)
EndFunction
