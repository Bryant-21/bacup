; Reading the bound BOOK is the story-event send. FO4's ObjectReference.OnRead fires
; for the player reading it from inventory or from the world, so GetContainer() is the
; reader when SendContainerAsRef2 is set -- which is what that FO76 name says, and the
; only second reference this script has to offer.
;
; The four cleanup properties (DeleteOnShutDown, DisableOnShutDown, DeleteOnDrop,
; DisableOnDrop) and SkipCleanupIfQuestNotStarted/bQuestStarted are deliberately left
; unimplemented: this is an ObjectReference script with no quest-shutdown hook, FO4
; exposes no remote quest-shutdown event to stand in for one, and the only carrier I
; could confirm (BOOK 39FC6F FFZ13_GiantTeapotAd, MyStoryManagerKeyword 10214D) leaves
; all four at their False defaults. A carrier that sets one needs its own evidence pass.
Event OnRead()
    ObjectReference containerRef = None
    If SendContainerAsRef2
        containerRef = GetContainer()
    EndIf
    SendConfiguredStoryEvent(containerRef)
EndEvent
